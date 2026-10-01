//! Current-text retained Markdown model shared by every editor host.

use std::ops::Range;

use crate::frontmatter::front_matter_span;
use crate::syntax::source_edit_may_change_global_semantics;
use crate::vim::TextEdit;

use super::blocks::{shift_block, Block, BlockKind, BlockModel, Inline, OwnedReferenceDefinition};

const MAX_LOCAL_BLOCK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct BlockDependencies {
    headings: usize,
    links: usize,
    footnote_references: Vec<String>,
    footnote_definitions: Vec<String>,
}

impl BlockDependencies {
    fn from_block(block: &Block) -> Self {
        fn visit_inlines(inlines: &[Inline], summary: &mut BlockDependencies) {
            for inline in inlines {
                match inline {
                    Inline::Link { text, .. } => {
                        summary.links += 1;
                        visit_inlines(text, summary);
                    }
                    Inline::Image { alt, .. } => visit_inlines(alt, summary),
                    Inline::Emph(children)
                    | Inline::Strong(children)
                    | Inline::Strike(children) => {
                        visit_inlines(children, summary);
                    }
                    Inline::FootnoteRef(leaf) => {
                        summary.footnote_references.push(leaf.text.clone());
                    }
                    Inline::Text(_)
                    | Inline::Code(_)
                    | Inline::SoftBreak(_)
                    | Inline::HardBreak(_)
                    | Inline::Html(_) => {}
                }
            }
        }

        fn visit(block: &Block, summary: &mut BlockDependencies) {
            match &block.kind {
                BlockKind::Heading { inlines, .. } => {
                    summary.headings += 1;
                    visit_inlines(inlines, summary);
                }
                BlockKind::Paragraph { inlines } => visit_inlines(inlines, summary),
                BlockKind::List { items, .. } => {
                    for item in items {
                        for child in &item.children {
                            visit(child, summary);
                        }
                    }
                }
                BlockKind::BlockQuote { children } => {
                    for child in children {
                        visit(child, summary);
                    }
                }
                BlockKind::FootnoteDef { label, children } => {
                    summary.footnote_definitions.push(label.clone());
                    for child in children {
                        visit(child, summary);
                    }
                }
                BlockKind::Table { header, rows, .. } => {
                    for cell in header {
                        visit_inlines(cell, summary);
                    }
                    for row in rows {
                        for cell in row {
                            visit_inlines(cell, summary);
                        }
                    }
                }
                BlockKind::FrontMatter
                | BlockKind::CodeFence { .. }
                | BlockKind::Rule
                | BlockKind::HtmlBlock { .. } => {}
            }
        }

        let mut summary = Self::default();
        visit(block, &mut summary);
        summary
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ModelWork {
    pub rebuilt_blocks: usize,
    pub reused_blocks: usize,
    pub parsed_bytes: usize,
    pub full_rebuild: bool,
}

pub(crate) enum ModelEditScope {
    Wide,
    FenceInterior {
        index: usize,
        old_span: Range<usize>,
        new_bracket_count: usize,
        line_delta: isize,
    },
    Local {
        index: usize,
        old_span: Range<usize>,
        new_bracket_count: usize,
        line_delta: isize,
    },
    Window {
        old_blocks: Range<usize>,
        old_span: Range<usize>,
        new_bracket_count: usize,
        line_delta: isize,
    },
}

/// Scope published by the synchronous Markdown-model mutation transaction.
pub(crate) enum ModelChange {
    Wide,
    FenceInterior {
        index: usize,
        edit_range: Range<usize>,
        new_text: String,
        source_delta: isize,
        line_delta: isize,
    },
    Local {
        index: usize,
        source_delta: isize,
        line_delta: isize,
        old_links: usize,
        new_links: usize,
    },
    Window {
        old_blocks: Range<usize>,
        new_blocks: Range<usize>,
        old_source_end: usize,
        source_delta: isize,
        line_delta: isize,
        old_links: usize,
        new_links: usize,
    },
}

/// A derived model, not another mutable owner of document text.
pub(crate) struct RetainedBlockModel {
    model: BlockModel,
    definitions: Vec<OwnedReferenceDefinition>,
    dependencies: Vec<BlockDependencies>,
    front_matter: Option<Range<usize>>,
    bracket_count: usize,
    work: ModelWork,
}

impl RetainedBlockModel {
    pub(crate) fn new(text: &str) -> Self {
        let front_matter = front_matter_span(text);
        let (model, definitions) =
            BlockModel::build_with_reference_definitions(text, front_matter.clone());
        let dependencies = model
            .blocks
            .iter()
            .map(BlockDependencies::from_block)
            .collect();
        let work = ModelWork {
            rebuilt_blocks: model.blocks.len(),
            reused_blocks: 0,
            parsed_bytes: text.len(),
            full_rebuild: true,
        };
        Self {
            model,
            definitions,
            dependencies,
            front_matter,
            bracket_count: text.bytes().filter(|byte| *byte == b'[').count(),
            work,
        }
    }

    pub(crate) fn model(&self) -> &BlockModel {
        &self.model
    }

    #[cfg(test)]
    pub(crate) fn work(&self) -> ModelWork {
        self.work
    }

    pub(crate) fn prepare_edit(&self, old_text: &str, edit: &TextEdit) -> ModelEditScope {
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        if edit.new_text_len == edit.new_text.len()
            && old_text.is_char_boundary(start)
            && old_text.is_char_boundary(end)
        {
            if let Some((index, block)) = self.model.blocks.iter().enumerate().find(|(_, block)| {
                matches!(&block.kind, BlockKind::CodeFence { content_span, indented: false, .. }
                    if content_span.start <= start && end <= content_span.end)
            }) {
                let old_line_start = old_text[..start].rfind('\n').map_or(0, |at| at + 1);
                let old_line_end = old_text[end..]
                    .find('\n')
                    .map_or(old_text.len(), |at| end + at);
                let replacement = format!(
                    "{}{}{}",
                    &old_text[old_line_start..start],
                    edit.new_text,
                    &old_text[end..old_line_end]
                );
                let looks_like_delimiter = |line: &str| {
                    let line = line.trim_start_matches([' ', '\t']);
                    line.starts_with("```") || line.starts_with("~~~")
                };
                if !old_text[old_line_start..old_line_end]
                    .lines()
                    .any(looks_like_delimiter)
                    && !replacement.lines().any(looks_like_delimiter)
                {
                    let old_newlines = old_text[start..end]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count();
                    let new_newlines = edit.new_text.bytes().filter(|byte| *byte == b'\n').count();
                    return ModelEditScope::FenceInterior {
                        index,
                        old_span: block.span.clone(),
                        new_bracket_count: self.bracket_count
                            - old_text[start..end]
                                .bytes()
                                .filter(|byte| *byte == b'[')
                                .count()
                            + edit.new_text.bytes().filter(|byte| *byte == b'[').count(),
                        line_delta: new_newlines as isize - old_newlines as isize,
                    };
                }
            }
        }
        let line_start = old_text[..start]
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        let line_end = old_text[end..]
            .find('\n')
            .map_or(old_text.len(), |newline| end + newline);
        let old_line = &old_text[line_start..line_end];
        let start_line_end = old_text[start..]
            .find('\n')
            .map_or(old_text.len(), |newline| start + newline);
        let start_line = &old_text[line_start..start_line_end];
        let new_line = format!(
            "{}{}{}",
            &old_text[line_start..start],
            edit.new_text,
            &old_text[end..line_end]
        );
        let extends_paragraph = start == end
            && old_line.trim().is_empty()
            && edit.new_text.ends_with('\n')
            && old_text[end..].starts_with('\n')
            && self.model.blocks.iter().any(|block| {
                block.span.end == start && matches!(block.kind, BlockKind::Paragraph { .. })
            });
        if ((start_line.trim().is_empty() || old_line.trim().is_empty()) && !extends_paragraph)
            || new_line.trim().is_empty()
            || self
                .front_matter
                .as_ref()
                .is_some_and(|span| start <= span.end && end >= span.start)
            || source_edit_may_change_global_semantics(old_text, edit)
            || old_line.contains("[^")
            || new_line.contains("[^")
            || self
                .definitions
                .iter()
                .any(|definition| start <= definition.span.end && end >= definition.span.start)
        {
            return ModelEditScope::Wide;
        }
        let Some(index) = self.model.blocks.iter().position(|block| {
            block.span.start <= start
                && (start < block.span.end || (extends_paragraph && start == block.span.end))
                && end <= block.span.end
        }) else {
            return ModelEditScope::Wide;
        };
        let block = &self.model.blocks[index];
        let old_newlines = old_text[start..end]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count();
        let new_newlines = edit.new_text.bytes().filter(|byte| *byte == b'\n').count();
        if matches!(
            block.kind,
            BlockKind::FrontMatter | BlockKind::FootnoteDef { .. }
        ) || !self.dependencies[index].footnote_references.is_empty()
            || (old_newlines != new_newlines
                && old_text[end..].starts_with('\n')
                && !extends_paragraph)
            || (old_newlines != new_newlines && !matches!(block.kind, BlockKind::Paragraph { .. }))
            || (old_newlines != new_newlines
                && matches!(block.kind, BlockKind::Paragraph { .. })
                && (old_line.contains('|') || new_line.contains('|')))
            || (old_newlines != new_newlines && index + 1 == self.model.blocks.len())
            || block.span.end - block.span.start > MAX_LOCAL_BLOCK_BYTES
            || self.definitions.iter().any(|definition| {
                definition.span.start < block.span.end && definition.span.end > block.span.start
            })
        {
            return ModelEditScope::Wide;
        }
        ModelEditScope::Local {
            index,
            old_span: block.span.clone(),
            new_bracket_count: self.bracket_count
                - old_text[start..end]
                    .bytes()
                    .filter(|byte| *byte == b'[')
                    .count()
                + edit.new_text.bytes().filter(|byte| *byte == b'[').count(),
            line_delta: new_newlines as isize - old_newlines as isize,
        }
    }

    pub(crate) fn prepare_window_edit(
        &self,
        old_text: &str,
        edit: &TextEdit,
        old_span: Range<usize>,
    ) -> Option<ModelEditScope> {
        if old_span.len() > MAX_LOCAL_BLOCK_BYTES
            || old_span.start > edit.range.start
            || edit.range.end > old_span.end
            || old_span.end > old_text.len()
            || !old_text.is_char_boundary(old_span.start)
            || !old_text.is_char_boundary(old_span.end)
            || self
                .front_matter
                .as_ref()
                .is_some_and(|span| old_span.start <= span.end)
            || self.definitions.iter().any(|definition| {
                definition.span.start < old_span.end && definition.span.end > old_span.start
            })
        {
            return None;
        }
        let first = self
            .model
            .blocks
            .partition_point(|block| block.span.end <= old_span.start);
        let last = self
            .model
            .blocks
            .partition_point(|block| block.span.start < old_span.end);
        if first >= last
            || self.model.blocks[first].span.start != old_span.start
            || !matches!(
                self.model.blocks[first].kind,
                BlockKind::Heading { level: 2..=6, .. }
            )
            || self.model.blocks[last - 1].span.end > old_span.end
            || self.dependencies[first..last].iter().any(|summary| {
                !summary.footnote_references.is_empty() || !summary.footnote_definitions.is_empty()
            })
        {
            return None;
        }
        let removed = &old_text[edit.range.clone()];
        let old_newlines = removed.bytes().filter(|byte| *byte == b'\n').count();
        let new_newlines = edit.new_text.bytes().filter(|byte| *byte == b'\n').count();
        Some(ModelEditScope::Window {
            old_blocks: first..last,
            old_span,
            new_bracket_count: self.bracket_count
                - removed.bytes().filter(|byte| *byte == b'[').count()
                + edit.new_text.bytes().filter(|byte| *byte == b'[').count(),
            line_delta: new_newlines as isize - old_newlines as isize,
        })
    }

    pub(crate) fn apply_edit(
        &mut self,
        text: &str,
        edit: &TextEdit,
        scope: ModelEditScope,
    ) -> ModelChange {
        if let ModelEditScope::FenceInterior {
            index,
            old_span,
            new_bracket_count,
            line_delta,
        } = scope
        {
            let delta = edit.new_text_len as isize - (edit.range.end - edit.range.start) as isize;
            let block = &mut self.model.blocks[index];
            block.span.end = old_span.end.checked_add_signed(delta).unwrap();
            let BlockKind::CodeFence { content_span, .. } = &mut block.kind else {
                unreachable!("fence scope names a code fence")
            };
            content_span.end = content_span.end.checked_add_signed(delta).unwrap();
            for block in &mut self.model.blocks[index + 1..] {
                if block.span.start >= old_span.end {
                    shift_block(block, delta);
                }
            }
            for definition in &mut self.definitions {
                if definition.span.start >= old_span.end {
                    definition.span.start =
                        definition.span.start.checked_add_signed(delta).unwrap();
                    definition.span.end = definition.span.end.checked_add_signed(delta).unwrap();
                }
            }
            self.bracket_count = new_bracket_count;
            self.work = ModelWork {
                rebuilt_blocks: 0,
                reused_blocks: self.model.blocks.len(),
                parsed_bytes: 0,
                full_rebuild: false,
            };
            return ModelChange::FenceInterior {
                index,
                edit_range: edit.range.clone(),
                new_text: edit.new_text.clone(),
                source_delta: delta,
                line_delta,
            };
        }
        if let ModelEditScope::Window {
            old_blocks,
            old_span,
            new_bracket_count,
            line_delta,
        } = scope
        {
            return self.apply_window_edit(
                text,
                edit,
                old_blocks,
                old_span,
                new_bracket_count,
                line_delta,
            );
        }
        let ModelEditScope::Local {
            index,
            old_span,
            new_bracket_count,
            line_delta,
        } = scope
        else {
            self.rebuild(text);
            return ModelChange::Wide;
        };
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        let delta = edit.new_text_len as isize - (end - start) as isize;
        let Some(new_end) = old_span.end.checked_add_signed(delta) else {
            self.rebuild(text);
            return ModelChange::Wide;
        };
        let new_span = old_span.start..new_end;
        if front_matter_span(text) != self.front_matter
            || new_span.end > text.len()
            || new_span.end - new_span.start > MAX_LOCAL_BLOCK_BYTES
            || !text.is_char_boundary(new_span.start)
            || !text.is_char_boundary(new_span.end)
        {
            self.rebuild(text);
            return ModelChange::Wide;
        }
        let Some(replacement) = BlockModel::build_range_with_reference_definitions(
            text,
            new_span.clone(),
            &self.definitions,
            self.definitions
                .iter()
                .map(|definition| definition.destination.len() + definition.title.len())
                .max()
                .unwrap_or(0)
                .checked_mul(new_bracket_count)
                .is_some_and(|upper_bound| upper_bound < 100_000),
        ) else {
            self.rebuild(text);
            return ModelChange::Wide;
        };
        if replacement.blocks.len() != 1
            || replacement.blocks[0].span.start != old_span.start
            || replacement.blocks[0].span.end != new_span.end
            || std::mem::discriminant(&replacement.blocks[0].kind)
                != std::mem::discriminant(&self.model.blocks[index].kind)
        {
            self.rebuild(text);
            return ModelChange::Wide;
        }
        let old_links = self.dependencies[index].links;
        self.model.blocks[index] = replacement.blocks.into_iter().next().unwrap();
        self.dependencies[index] = BlockDependencies::from_block(&self.model.blocks[index]);
        let new_links = self.dependencies[index].links;
        for block in &mut self.model.blocks[index + 1..] {
            if block.span.start >= old_span.end {
                shift_block(block, delta);
            }
        }
        for definition in &mut self.definitions {
            if definition.span.start >= old_span.end {
                definition.span.start = definition.span.start.checked_add_signed(delta).unwrap();
                definition.span.end = definition.span.end.checked_add_signed(delta).unwrap();
            }
        }
        self.bracket_count = new_bracket_count;
        self.work = ModelWork {
            rebuilt_blocks: 1,
            reused_blocks: self.model.blocks.len() - 1,
            parsed_bytes: new_span.end - new_span.start,
            full_rebuild: false,
        };
        ModelChange::Local {
            index,
            source_delta: delta,
            line_delta,
            old_links,
            new_links,
        }
    }

    fn apply_window_edit(
        &mut self,
        text: &str,
        edit: &TextEdit,
        old_blocks: Range<usize>,
        old_span: Range<usize>,
        new_bracket_count: usize,
        line_delta: isize,
    ) -> ModelChange {
        let delta = edit.new_text_len as isize - edit.range.len() as isize;
        let Some(new_end) = old_span.end.checked_add_signed(delta) else {
            self.rebuild(text);
            return ModelChange::Wide;
        };
        let new_span = old_span.start..new_end;
        if front_matter_span(text) != self.front_matter
            || new_span.len() > MAX_LOCAL_BLOCK_BYTES
            || new_span.end > text.len()
            || !text.is_char_boundary(new_span.end)
        {
            self.rebuild(text);
            return ModelChange::Wide;
        }
        let reference_budget_safe = self
            .definitions
            .iter()
            .map(|definition| definition.destination.len() + definition.title.len())
            .max()
            .unwrap_or(0)
            .checked_mul(new_bracket_count)
            .is_some_and(|upper_bound| upper_bound < 100_000);
        let Some(replacement) = BlockModel::build_range_with_reference_definitions(
            text,
            new_span.clone(),
            &self.definitions,
            reference_budget_safe,
        ) else {
            self.rebuild(text);
            return ModelChange::Wide;
        };
        if replacement.blocks.first().is_none_or(|block| {
            block.span.start != new_span.start
                || !matches!(block.kind, BlockKind::Heading { level: 2..=6, .. })
        }) || replacement
            .blocks
            .last()
            .is_none_or(|block| block.span.end > new_span.end)
        {
            self.rebuild(text);
            return ModelChange::Wide;
        }
        let old_links = self.dependencies[old_blocks.clone()]
            .iter()
            .map(|summary| summary.links)
            .sum();
        let new_dependencies = replacement
            .blocks
            .iter()
            .map(BlockDependencies::from_block)
            .collect::<Vec<_>>();
        let new_links = new_dependencies.iter().map(|summary| summary.links).sum();
        let new_count = replacement.blocks.len();
        let first = old_blocks.start;
        self.model
            .blocks
            .splice(old_blocks.clone(), replacement.blocks);
        self.dependencies
            .splice(old_blocks.clone(), new_dependencies);
        for block in &mut self.model.blocks[first + new_count..] {
            if block.span.start >= old_span.end {
                shift_block(block, delta);
            }
        }
        for definition in &mut self.definitions {
            if definition.span.start >= old_span.end {
                definition.span.start = definition.span.start.checked_add_signed(delta).unwrap();
                definition.span.end = definition.span.end.checked_add_signed(delta).unwrap();
            }
        }
        self.bracket_count = new_bracket_count;
        self.work = ModelWork {
            rebuilt_blocks: new_count,
            reused_blocks: self.model.blocks.len() - new_count,
            parsed_bytes: new_span.len(),
            full_rebuild: false,
        };
        ModelChange::Window {
            old_blocks,
            new_blocks: first..first + new_count,
            old_source_end: old_span.end,
            source_delta: delta,
            line_delta,
            old_links,
            new_links,
        }
    }

    fn rebuild(&mut self, text: &str) {
        *self = Self::new(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::realistic_fixtures as fixtures;
    use proptest::prelude::*;

    #[test]
    #[ignore = "exact 1 MiB multi-block window feasibility diagnostic"]
    fn acceptance_1mb_local_model_windows_match_full_builder() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        assert_eq!(original.len(), 1_048_722);
        for (first_line, ranges) in [
            (600, vec![18003..18518, 18524..18551, 18556..18557]),
            (130, std::iter::once(3948..4243).collect()),
        ] {
            let retained = RetainedBlockModel::new(&original);
            let old_window_start = original[..ranges[0].start]
                .rfind("\n## ")
                .map_or(0, |at| at + 1);
            let old_window_end = original[ranges.last().unwrap().end..]
                .find("\n## ")
                .map_or(original.len(), |at| ranges.last().unwrap().end + at + 1);
            let mut current = original.clone();
            for range in ranges.iter().rev() {
                current.replace_range(range.clone(), "");
            }
            let removed_bytes = ranges
                .iter()
                .map(|range| range.end - range.start)
                .sum::<usize>();
            let new_window = old_window_start..old_window_end - removed_bytes;
            let started = std::time::Instant::now();
            let local = BlockModel::build_range_with_reference_definitions(
                &current,
                new_window.clone(),
                &retained.definitions,
                true,
            )
            .unwrap();
            let local_ns = started.elapsed().as_nanos();
            let fresh = RetainedBlockModel::new(&current);
            let expected = fresh
                .model
                .blocks
                .iter()
                .filter(|block| {
                    new_window.start <= block.span.start && block.span.end <= new_window.end
                })
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(local.blocks, expected);
            assert!(old_window_end - old_window_start < 4096);
            println!(
                "PROSE-MODEL-WINDOW\t{first_line}\t{}\t{}\t{local_ns}",
                new_window.end - new_window.start,
                local.blocks.len(),
            );
        }
    }

    #[test]
    fn exact_prose_and_list_section_windows_retain_complete_model() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        for ranges in [
            vec![18003..18518, 18524..18551, 18556..18557],
            std::iter::once(3948..4243).collect(),
        ] {
            let edits = ranges
                .into_iter()
                .rev()
                .map(|range| TextEdit {
                    range,
                    new_text_len: 0,
                    new_text: String::new(),
                })
                .collect::<Vec<_>>();
            let edit = crate::vim::collapsed_descending_edit(&original, &edits).unwrap();
            let source = crate::syntax::Highlighter::new(&original);
            let window = source.local_section_window(&edit).unwrap();
            let mut retained = RetainedBlockModel::new(&original);
            let scope = retained
                .prepare_window_edit(&original, &edit, window)
                .unwrap();
            let mut current = original.clone();
            current.replace_range(edit.range.clone(), &edit.new_text);
            assert!(matches!(
                retained.apply_edit(&current, &edit, scope),
                ModelChange::Window { .. }
            ));
            assert_eq!(retained.model(), RetainedBlockModel::new(&current).model());
            assert!(!retained.work().full_rebuild);
            assert!(retained.work().parsed_bytes < 4096);
        }
    }

    #[test]
    fn interior_fence_function_delete_and_undo_keep_complete_model() {
        for language in ["rust", "go"] {
            let mut text = format!("# Heading\n\n```{language}\n");
            for index in 0..4000 {
                text.push_str(&format!("fn item_{index}() {{ let value = {index}; }}\n"));
            }
            text.push_str("```\n\nTail paragraph.\n");
            let original = text.clone();
            let mut retained = RetainedBlockModel::new(&text);
            let start = text.find("fn item_2000").unwrap();
            let end = text[start..]
                .split_inclusive('\n')
                .take(15)
                .map(str::len)
                .sum::<usize>()
                + start;
            let removed = text[start..end].to_string();
            let delete = TextEdit {
                range: start..end,
                new_text_len: 0,
                new_text: String::new(),
            };
            let scope = retained.prepare_edit(&text, &delete);
            assert!(matches!(scope, ModelEditScope::FenceInterior { .. }));
            text.replace_range(start..end, "");
            assert!(matches!(
                retained.apply_edit(&text, &delete, scope),
                ModelChange::FenceInterior {
                    line_delta: -15,
                    ..
                }
            ));
            assert_eq!(retained.work().parsed_bytes, 0);
            assert_eq!(retained.model(), &RetainedBlockModel::new(&text).model);
            let restore = TextEdit {
                range: start..start,
                new_text_len: removed.len(),
                new_text: removed.clone(),
            };
            let scope = retained.prepare_edit(&text, &restore);
            assert!(matches!(scope, ModelEditScope::FenceInterior { .. }));
            text.insert_str(start, &removed);
            retained.apply_edit(&text, &restore, scope);
            assert_eq!(text, original);
            assert_eq!(retained.model(), &RetainedBlockModel::new(&text).model);
            let delimiter_start = text.find(&format!("```{language}")).unwrap();
            let delimiter = TextEdit {
                range: delimiter_start..delimiter_start + 3,
                new_text_len: 0,
                new_text: String::new(),
            };
            assert!(matches!(
                retained.prepare_edit(&text, &delimiter),
                ModelEditScope::Wide
            ));
        }
    }

    #[test]
    fn fence_body_boundary_characters_stay_local_when_delimiters_do_not_change() {
        for language in ["rust", "go"] {
            let mut text = format!("# Title\n\n```{language}\nalpha();\nbeta();\n```\n\nTail.\n");
            let mut retained = RetainedBlockModel::new(&text);
            for (needle, replacement) in [("alpha", "A"), ("beta();", "!")] {
                let at = text.find(needle).unwrap();
                let range = if needle == "alpha" {
                    at..at + 1
                } else {
                    at + needle.len() - 1..at + needle.len()
                };
                let edit = TextEdit {
                    range: range.clone(),
                    new_text_len: replacement.len(),
                    new_text: replacement.to_string(),
                };
                let scope = retained.prepare_edit(&text, &edit);
                assert!(matches!(scope, ModelEditScope::FenceInterior { .. }));
                text.replace_range(range, replacement);
                assert!(matches!(
                    retained.apply_edit(&text, &edit, scope),
                    ModelChange::FenceInterior { .. }
                ));
                assert_eq!(retained.model(), &RetainedBlockModel::new(&text).model);
            }
        }
    }

    fn prototype_local_fence_delete(
        retained: &mut RetainedBlockModel,
        text: &str,
        range: Range<usize>,
    ) -> usize {
        let index = retained
            .model
            .blocks
            .iter()
            .position(|block| {
                matches!(&block.kind, BlockKind::CodeFence { content_span, indented: false, .. }
                    if content_span.start < range.start && range.end < content_span.end)
            })
            .expect("edit must stay inside a fenced code body");
        let deleted = &text[range.clone()];
        assert!(deleted.ends_with('\n'));
        assert!(!deleted.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("```") || line.starts_with("~~~")
        }));
        let delta = -(deleted.len() as isize);
        let block = &mut retained.model.blocks[index];
        block.span.end = block.span.end.checked_add_signed(delta).unwrap();
        let BlockKind::CodeFence { content_span, .. } = &mut block.kind else {
            unreachable!()
        };
        content_span.end = content_span.end.checked_add_signed(delta).unwrap();
        for block in &mut retained.model.blocks[index + 1..] {
            shift_block(block, delta);
        }
        for definition in &mut retained.definitions {
            if definition.span.start >= range.end {
                definition.span.start = definition.span.start.checked_add_signed(delta).unwrap();
                definition.span.end = definition.span.end.checked_add_signed(delta).unwrap();
            }
        }
        retained.bracket_count -= deleted.bytes().filter(|byte| *byte == b'[').count();
        retained.model.blocks.len() - index - 1
    }

    #[test]
    #[ignore = "exact 1 MiB fence-model feasibility diagnostic is run by its benchmark target"]
    fn acceptance_1mb_fence_model_splice_matches_full_builder() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        for (language, first_line) in [("rust", 3000), ("go", 20000)] {
            let start = original
                .split_inclusive('\n')
                .take(first_line)
                .map(str::len)
                .sum::<usize>();
            let end = start
                + original[start..]
                    .split_inclusive('\n')
                    .take(15)
                    .map(str::len)
                    .sum::<usize>();
            let mut retained = RetainedBlockModel::new(&original);
            let started = std::time::Instant::now();
            let shifted_blocks = prototype_local_fence_delete(&mut retained, &original, start..end);
            let splice_ns = started.elapsed().as_nanos();
            let edited = format!("{}{}", &original[..start], &original[end..]);
            let started = std::time::Instant::now();
            let complete = BlockModel::build(&edited, front_matter_span(&edited));
            let complete_ns = started.elapsed().as_nanos();
            assert_eq!(retained.model, complete);
            let fresh = RetainedBlockModel::new(&edited);
            assert_eq!(retained.definitions, fresh.definitions);
            assert_eq!(retained.dependencies, fresh.dependencies);
            assert_eq!(retained.front_matter, fresh.front_matter);
            assert_eq!(retained.bracket_count, fresh.bracket_count);
            println!("FENCE_MODEL\t{language}\t{splice_ns}\t{complete_ns}\t{shifted_blocks}");
        }
    }

    const MODEL_CASES: [(&str, &str); 9] = [
        ("front-matter", "---\ntitle: Café\n---\n\nBody text.\n"),
        (
            "paragraph",
            "# Heading\n\nA \\*literal\\* &amp; duplicate duplicate.\n",
        ),
        ("list", "> - nested item\n> - second item\n\nTail.\n"),
        ("table", "| A | B |\n|---|---|\n| α | `β` |\n"),
        ("fence", "```rust\nfn main() { let x = 1; }\n```\n"),
        ("reference", "[ref]: /old\n\nA [link][ref] here.\n"),
        ("footnote", "[^n]: note body\n\nA [^n] and tail.\n"),
        ("crlf", "+++\r\ntitle = 'Note'\r\n+++\r\n\r\n# α\r\n"),
        ("html", "<div>\ninside html\n</div>\n\nTail.\n"),
    ];

    #[test]
    fn model_case_inventory_covers_every_required_construct() {
        assert_eq!(
            MODEL_CASES.map(|(name, _)| name),
            [
                "front-matter",
                "paragraph",
                "list",
                "table",
                "fence",
                "reference",
                "footnote",
                "crlf",
                "html",
            ]
        );
        assert!(MODEL_CASES.iter().all(|(_, body)| !body.is_empty()));
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 2048,
            rng_seed: proptest::test_runner::RngSeed::Fixed(0x09ed_cafe),
            failure_persistence: None,
            ..ProptestConfig::default()
        })]

        #[test]
        fn random_edits_and_undo_match_full_model(
            case in 0usize..MODEL_CASES.len(),
            start_choice in any::<u8>(),
            removed_chars in 0usize..3,
            replacement in prop::sample::select(vec!["", "x", "\n", "\r\n", "`", "**", "é", "#", "| x |", "---\n", "<div>\n", "[ref]: /new\n", "[^n]"]),
        ) {
            let before = MODEL_CASES[case].1;
            let boundaries = before
                .char_indices()
                .map(|(offset, _)| offset)
                .chain([before.len()])
                .collect::<Vec<_>>();
            let index = start_choice as usize % boundaries.len();
            let start = boundaries[index];
            let end = boundaries[(index + removed_chars).min(boundaries.len() - 1)];
            let replacement = if start == end && replacement.is_empty() { "x" } else { replacement };
            let removed = before[start..end].to_string();
            let mut text = before.to_string();
            let mut retained = RetainedBlockModel::new(&text);
            apply_and_compare(&mut retained, &mut text, TextEdit {
                range: start..end,
                new_text_len: replacement.len(),
                new_text: replacement.to_string(),
            });
            apply_and_compare(&mut retained, &mut text, TextEdit {
                range: start..start + replacement.len(),
                new_text_len: removed.len(),
                new_text: removed,
            });
            prop_assert_eq!(text, before);
        }
    }

    fn apply_and_compare(retained: &mut RetainedBlockModel, text: &mut String, edit: TextEdit) {
        let scope = retained.prepare_edit(text, &edit);
        let start = edit.range.start.min(edit.range.end);
        let end = edit.range.start.max(edit.range.end);
        text.replace_range(start..end, &edit.new_text);
        retained.apply_edit(text, &edit, scope);
        let (fresh, definitions) =
            BlockModel::build_with_reference_definitions(text, front_matter_span(text));
        assert_eq!(
            retained.model.blocks.len(),
            fresh.blocks.len(),
            "edit {:?} -> {:?} in {text:?}",
            edit.range,
            edit.new_text
        );
        for (index, (actual, expected)) in
            retained.model.blocks.iter().zip(&fresh.blocks).enumerate()
        {
            assert_eq!(actual, expected, "block {index}, edit {:?}", edit.new_text);
        }
        assert_eq!(retained.definitions, definitions);
        assert_eq!(retained.front_matter, front_matter_span(text));
        assert_eq!(
            retained.dependencies,
            fresh
                .blocks
                .iter()
                .map(BlockDependencies::from_block)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn container_and_adjacent_block_boundaries_match_full_parser() {
        let documents = [
            "> first line\nlazy continuation\n> second line\n\nTail.\n",
            "> - first item\n>   continuation\n> - second item\n\nTail.\n",
            "1. first item\n   continuation\n2. second item\n\nTail.\n",
            "- first item\n\n  second paragraph\n- second item\n\nTail.\n",
            "A paragraph\ncontinued here\n\nAnother paragraph.\n",
            "Heading\n=======\n\nA paragraph.\n",
            "| A | B |\n|---|---|\n| x | y |\n\nTail.\n",
            "```rust\nlet x = 1;\n```\n\nTail.\n",
            "<div>\ninside\n</div>\n\nTail.\n",
            "A [link][ref] and [another][ref].\n\n[ref]: /target\n\nTail.\n",
        ];
        for before in documents {
            let boundaries = before
                .char_indices()
                .map(|(offset, _)| offset)
                .chain([before.len()])
                .collect::<Vec<_>>();
            for (index, &start) in boundaries.iter().enumerate() {
                for replacement in ["", " ", "\n", "\r\n", "x", "-", ">", "`", "|", "[^"] {
                    let end = if replacement.is_empty() {
                        boundaries[(index + 1).min(boundaries.len() - 1)]
                    } else {
                        start
                    };
                    if start == end && replacement.is_empty() {
                        continue;
                    }
                    let mut text = before.to_string();
                    let mut retained = RetainedBlockModel::new(&text);
                    apply_and_compare(
                        &mut retained,
                        &mut text,
                        TextEdit {
                            range: start..end,
                            new_text_len: replacement.len(),
                            new_text: replacement.to_string(),
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn edits_to_blank_separators_rebuild_neighboring_blocks() {
        let mut list = "1. first item\n   continuation\n2. second item\n\nTail.\n".to_string();
        let mut retained = RetainedBlockModel::new(&list);
        let insert = list.find("\n\nTail.").unwrap() + 1;
        apply_and_compare(
            &mut retained,
            &mut list,
            TextEdit {
                range: insert..insert,
                new_text_len: 1,
                new_text: "x".to_string(),
            },
        );
        assert!(retained.work.full_rebuild);

        let mut paragraphs = "A paragraph\ncontinued here\n\nAnother paragraph.\n".to_string();
        let mut retained = RetainedBlockModel::new(&paragraphs);
        let separator = paragraphs.find("\n\nAnother").unwrap();
        apply_and_compare(
            &mut retained,
            &mut paragraphs,
            TextEdit {
                range: separator..separator + 1,
                new_text_len: 0,
                new_text: String::new(),
            },
        );
        assert!(retained.work.full_rebuild);
    }

    #[test]
    fn local_reference_link_keeps_full_parser_destination_and_exact_atoms() {
        let mut text = "[Ref]: https://example.invalid/destination\n\nA [link][rEf] and repeated repeated text.\n\nTail.\n".to_string();
        let mut retained = RetainedBlockModel::new(&text);
        let start = text.find("repeated").unwrap();
        apply_and_compare(
            &mut retained,
            &mut text,
            TextEdit {
                range: start..start + 8,
                new_text_len: 9,
                new_text: "rewritten".to_string(),
            },
        );
        assert!(!retained.work.full_rebuild);
        assert_eq!(retained.work.rebuilt_blocks, 1);
        assert_eq!(retained.work.reused_blocks, 1);
    }

    #[test]
    fn definition_change_rebuilds_all_link_dependents() {
        let mut text =
            "[ref]: https://example.invalid/old\n\nA [link][ref].\n\nA second [link][REF].\n"
                .to_string();
        let mut retained = RetainedBlockModel::new(&text);
        let start = text.find("/old").unwrap();
        apply_and_compare(
            &mut retained,
            &mut text,
            TextEdit {
                range: start + 1..start + 4,
                new_text_len: 3,
                new_text: "new".to_string(),
            },
        );
        assert!(retained.work.full_rebuild);
        assert_eq!(retained.work.reused_blocks, 0);
    }

    #[test]
    fn local_change_reports_geometry_and_link_count_to_row_projection() {
        let mut text = "# Heading\n\nA [link](https://example.invalid) and another line.\nSecond line.\nThird line.\n\nTail.\n".to_string();
        let mut retained = RetainedBlockModel::new(&text);
        let start = text.find("Second line.").unwrap();
        let end = start + "Second line.\n".len();
        let edit = TextEdit {
            range: start..end,
            new_text_len: 0,
            new_text: String::new(),
        };
        let scope = retained.prepare_edit(&text, &edit);
        text.replace_range(start..end, "");
        let change = retained.apply_edit(&text, &edit, scope);
        assert!(matches!(
            change,
            ModelChange::Local {
                index: 1,
                source_delta,
                line_delta: -1,
                old_links: 1,
                new_links: 1,
            } if source_delta == -((end - start) as isize)
        ));
        assert_eq!(
            retained.model(),
            &BlockModel::build(&text, front_matter_span(&text))
        );
    }

    #[test]
    fn one_megabyte_mixed_line_delete_and_undo_reuses_unchanged_blocks() {
        let before = fixtures::generate("mixed", 1024 * 1024);
        let middle_line = before
            .lines()
            .enumerate()
            .skip(before.lines().count() / 2)
            .find_map(|(line, content)| content.starts_with("## ").then_some(line + 2))
            .unwrap();
        for line in [6, middle_line] {
            let mut text = before.clone();
            let mut retained = RetainedBlockModel::new(&text);
            let start = text
                .split_inclusive('\n')
                .take(line)
                .map(str::len)
                .sum::<usize>();
            let end = start + text[start..].find('\n').unwrap() + 1;
            let removed = text[start..end].to_string();
            apply_and_compare(
                &mut retained,
                &mut text,
                TextEdit {
                    range: start..end,
                    new_text_len: 0,
                    new_text: String::new(),
                },
            );
            assert!(!retained.work.full_rebuild, "line {line}");
            assert!(retained.work.reused_blocks > 1000, "line {line}");
            assert!(retained.work.parsed_bytes <= MAX_LOCAL_BLOCK_BYTES);
            apply_and_compare(
                &mut retained,
                &mut text,
                TextEdit {
                    range: start..start,
                    new_text_len: removed.len(),
                    new_text: removed,
                },
            );
            assert_eq!(text, before);
            assert!(!retained.work.full_rebuild, "undo line {line}");
        }
    }

    #[test]
    fn construct_edits_and_undo_match_full_model_with_crlf_and_unicode() {
        let body = concat!(
            "---\ntitle: Original\n---\n\n",
            "# Heading Café\n\n",
            "[ref]: https://example.invalid/old\n\n",
            "A [link][ref], &amp;, \\*escaped\\*, and duplicate duplicate.\n\n",
            "> - nested itemword\n> - second item\n\n",
            "| A | B |\n|---|---|\n| α | tableword |\n\n",
            "```unknown\nplain codeword\n```\n\n",
            "<div>\ninside htmlword\n</div>\n\n",
            "[^note]: footnoteword\n\nA [^note] and tailword.\n",
        );
        let mut text = body.replace('\n', "\r\n");
        let mut retained = RetainedBlockModel::new(&text);
        for (needle, replacement, wide) in [
            ("Original", "Revised", true),
            ("Café", "Résumé", false),
            ("/old", "/new", true),
            ("duplicate", "repeated", false),
            ("itemword", "itemname", false),
            ("tableword", "tablecell", false),
            ("unknown", "rust", false),
            ("codeword", "codevalue", false),
            ("htmlword", "htmlvalue", false),
            ("footnoteword", "footnotebody", true),
            ("tailword", "tailvalue", false),
            ("<div>", "plain", true),
            ("```unknown", "``unknown", true),
        ] {
            let start = text.find(needle).unwrap();
            let removed = text[start..start + needle.len()].to_string();
            apply_and_compare(
                &mut retained,
                &mut text,
                TextEdit {
                    range: start..start + needle.len(),
                    new_text_len: replacement.len(),
                    new_text: replacement.to_string(),
                },
            );
            if wide {
                assert!(retained.work.full_rebuild, "{needle}");
            }
            apply_and_compare(
                &mut retained,
                &mut text,
                TextEdit {
                    range: start..start + replacement.len(),
                    new_text_len: removed.len(),
                    new_text: removed,
                },
            );
            assert_eq!(text, body.replace('\n', "\r\n"));
        }
    }

    #[test]
    fn distant_one_megabyte_edits_and_reverse_undo_keep_spans_current() {
        const SEED: u64 = 0x9eed_cafe_1234_5678;
        let mut seed = SEED;
        let before = fixtures::generate("mixed", 1024 * 1024);
        let mut text = before.clone();
        let mut retained = RetainedBlockModel::new(&text);
        let mut undo = Vec::new();
        for step in 0..4 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let prose_lines = text
                .match_indices("A paragraph with")
                .map(|(offset, _)| offset)
                .collect::<Vec<_>>();
            let start = prose_lines[(seed as usize) % prose_lines.len()];
            let end = start + text[start..].find('\n').unwrap() + 1;
            let removed = text[start..end].to_string();
            apply_and_compare(
                &mut retained,
                &mut text,
                TextEdit {
                    range: start..end,
                    new_text_len: 0,
                    new_text: String::new(),
                },
            );
            assert!(!retained.work.full_rebuild, "seed {SEED:x}, step {step}");
            undo.push((start, removed));
        }
        for (step, (start, removed)) in undo.into_iter().rev().enumerate() {
            apply_and_compare(
                &mut retained,
                &mut text,
                TextEdit {
                    range: start..start,
                    new_text_len: removed.len(),
                    new_text: removed,
                },
            );
            assert!(!retained.work.full_rebuild, "seed {SEED:x}, undo {step}");
        }
        assert_eq!(text, before);
    }

    #[test]
    fn unicode_reference_falls_back_to_parser_case_folding() {
        let mut text = "[K]: /kelvin\n\nA [link][k] and a word.\n".to_string();
        let mut retained = RetainedBlockModel::new(&text);
        let start = text.find("word").unwrap();
        apply_and_compare(
            &mut retained,
            &mut text,
            TextEdit {
                range: start..start + 4,
                new_text_len: 5,
                new_text: "words".to_string(),
            },
        );
        assert!(retained.work.full_rebuild);
    }

    #[test]
    fn reference_expansion_budget_uses_full_parser_when_local_budget_can_differ() {
        let mut text = format!("[ref]: /{}\n\n", "a".repeat(200));
        text.push_str(&"A [link][ref] word.\n\n".repeat(1000));
        let mut retained = RetainedBlockModel::new(&text);
        let start = text.find("word").unwrap();
        apply_and_compare(
            &mut retained,
            &mut text,
            TextEdit {
                range: start..start + 4,
                new_text_len: 5,
                new_text: "words".to_string(),
            },
        );
        assert!(retained.work.full_rebuild);
    }
}
