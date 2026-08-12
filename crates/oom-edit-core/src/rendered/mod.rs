//! rendered layout renderer — produces styled, wrapped, source-mapped rendered lines.
//!
//! `RenderedLayout::build(&BlockModel, width, &Highlighter)` implements every VW
//! row (VW-1..VW-14) from plan §6.3.2, plus the front-matter panel (FR-3.6).
//!
//! See architecture §6.4 for the pipeline description.
//!
//! ## Inline styling rule
//!
//! When mapping `Inline` nodes to `SemanticStyle` spans, nested emphasis
//! resolves to the innermost semantic. For example, `Strong(Emph(..))`
//! resolves to `Strong` — the core does not compose modifier stacks.
//! Real modifier composition happens theme-side in the TUI. This rule is
//! documented here so implementers know the core's behavior is intentionally
//! flat: each inline node carries exactly one `SemanticStyle`.

mod blocks;
pub mod nav;
mod table;
mod wrap;

use std::ops::Range;
use unicode_width::UnicodeWidthChar;

use crate::style::{
    JumpTarget, LineKind, RenderedLayout, RenderedLine, RenderedLineRole, SemanticStyle, Span,
    StyledLine, TargetKind,
};
use crate::syntax;
use wrap::{wrap_mapped_line, MappedLine};

// ── Public API ─────────────────────────────────────────────────────────────

impl RenderedLayout {
    /// Build a `RenderedLayout` from a `BlockModel`, layout width, and highlighter.
    ///
    /// Produces styled, wrapped, source-mapped rendered lines implementing
    /// every VW row (VW-1..VW-14) and the front-matter panel (FR-3.6).
    ///
    /// Blank-line policy: exactly one synthetic blank line between top-level
    /// blocks; no leading blank before the first line; no trailing blank after
    /// the last.
    #[cfg(test)]
    pub(crate) fn build(model: &BlockModel, width: u16, highlighter: &syntax::Highlighter) -> Self {
        Self::build_with_front_matter_state(model, width, highlighter, false)
    }

    /// Build a layout while controlling the front-matter panel's collapsed state.
    pub(crate) fn build_with_front_matter_state(
        model: &BlockModel,
        width: u16,
        highlighter: &syntax::Highlighter,
        front_matter_collapsed: bool,
    ) -> Self {
        if width == 0 {
            return Self::default();
        }

        let layout = RenderedLayoutBuilder::new(model, width, highlighter, front_matter_collapsed);
        layout.build()
    }
}

// ── Builder ────────────────────────────────────────────────────────────────

struct RenderedLayoutBuilder<'a> {
    model: &'a BlockModel,
    width: u16,
    highlighter: &'a syntax::Highlighter,
    lines: Vec<RenderedLine>,
    jump_targets: Vec<JumpTarget>,
    link_index: Vec<(usize, String)>,
    last_content_source: Option<Range<usize>>,
    next_link_marker: usize,
    footnote_defs: Vec<FootnoteDef>,
    front_matter_collapsed: bool,
}

impl<'a> RenderedLayoutBuilder<'a> {
    fn new(
        model: &'a BlockModel,
        width: u16,
        highlighter: &'a syntax::Highlighter,
        front_matter_collapsed: bool,
    ) -> Self {
        Self {
            model,
            width,
            highlighter,
            lines: Vec::new(),
            jump_targets: Vec::new(),
            link_index: Vec::new(),
            last_content_source: None,
            next_link_marker: 0,
            footnote_defs: Vec::new(),
            front_matter_collapsed,
        }
    }

    fn build(mut self) -> RenderedLayout {
        // Process top-level blocks
        for (i, block) in self.model.blocks.iter().enumerate() {
            if i > 0 && !self.lines.is_empty() {
                // Blank line between blocks
                self.add_synthetic_blank(block.span.clone());
            }
            self.render_block(block);
        }

        // Append footnote definitions at document end
        self.append_footnotes();

        // Append link index at document end (if any links were found)
        if !self.link_index.is_empty() {
            self.add_synthetic_blank(Range { start: 0, end: 0 });
            self.append_link_index();
        }

        let line_numbers = rendered_line_numbers(&self.lines, self.highlighter.text());
        RenderedLayout {
            lines: self.lines,
            line_numbers,
            jump_targets: self.jump_targets,
            link_index: self.link_index,
        }
    }

    fn add_synthetic_blank(&mut self, source: Range<usize>) {
        let source = self.last_content_source.clone().unwrap_or(source);
        self.lines.push(RenderedLine {
            styled: StyledLine {
                text: String::new(),
                spans: Vec::new(),
            },
            source: source.clone(),
            kind: LineKind::Synthetic,
            role: RenderedLineRole::Document,
            atoms: Vec::new(),
        });
    }

    fn set_last_content_source(&mut self, source: Range<usize>) {
        self.last_content_source = Some(source);
    }

    fn make_content_line(&mut self, mapped: MappedLine, source: Range<usize>) {
        let (styled, atoms) = mapped.into_parts();
        self.lines.push(RenderedLine {
            styled,
            source: source.clone(),
            kind: LineKind::Content,
            role: RenderedLineRole::Document,
            atoms,
        });
        self.set_last_content_source(source);
    }

    fn make_synthetic_line(&mut self, mapped: MappedLine, source: Range<usize>) {
        let source = self.last_content_source.clone().unwrap_or(source);
        let (styled, atoms) = mapped.into_parts();
        self.lines.push(RenderedLine {
            styled,
            source,
            kind: LineKind::Synthetic,
            role: RenderedLineRole::Document,
            atoms,
        });
    }

    fn make_generated_content_line(&mut self, styled: StyledLine, source: Range<usize>) {
        self.make_content_line(mapped_from_styled(styled, None), source);
    }

    fn make_source_content_line(
        &mut self,
        styled: StyledLine,
        source: Range<usize>,
        visible_start: usize,
    ) {
        self.make_content_line(mapped_from_styled(styled, Some(visible_start)), source);
    }

    fn make_synthetic_styled_line(&mut self, styled: StyledLine, source: Range<usize>) {
        self.make_synthetic_line(mapped_from_styled(styled, None), source);
    }

    fn register_link(&mut self, dest: String) -> usize {
        let marker = self.next_link_marker;
        self.next_link_marker += 1;
        self.link_index.push((marker, dest));
        marker
    }

    // ── Block renderers ──────────────────────────────────────────────

    fn render_block(&mut self, block: &Block) {
        match &block.kind {
            BlockKind::FrontMatter => self.render_front_matter(block),
            BlockKind::Heading { level, inlines } => {
                self.render_heading(*level, inlines, &block.span)
            }
            BlockKind::Paragraph { inlines } => self.render_paragraph(inlines, &block.span),
            BlockKind::CodeFence {
                lang,
                content_span,
                indented,
            } => self.render_code_fence(lang, content_span, *indented, &block.span),
            BlockKind::List {
                ordered,
                tight: _tight,
                items,
            } => self.render_list(*ordered, items),
            BlockKind::BlockQuote { children } => self.render_blockquote(children, &block.span),
            BlockKind::Table {
                alignments,
                header,
                rows,
            } => self.render_table(alignments, header, rows, &block.span),
            BlockKind::Rule => self.render_rule(&block.span),
            BlockKind::HtmlBlock { content_span } => {
                self.render_html_block(content_span, &block.span)
            }
            BlockKind::FootnoteDef { label, children } => {
                self.render_footnote_def(label, children, &block.span)
            }
        }
    }

    fn render_block_into(
        &mut self,
        block: &Block,
        prefix: &str,
        available_width: u16,
        prefix_style: Option<SemanticStyle>,
    ) {
        let parent_width = self.width;
        let first_child_line = self.lines.len();
        self.width = available_width;
        self.render_block(block);
        self.width = parent_width;

        let prefix_char_len = prefix.chars().count();
        let prefix_width = text_width(prefix);
        for rendered_line in &mut self.lines[first_child_line..] {
            rendered_line.styled.text.insert_str(0, prefix);
            for atom in &mut rendered_line.atoms {
                atom.columns.start += prefix_width;
                atom.columns.end += prefix_width;
            }
            let mut prefix_atoms = display_groups(prefix)
                .into_iter()
                .scan(0, |column, group| {
                    let width = text_width(&group);
                    let atom = crate::style::RenderedSourceAtom {
                        columns: *column..*column + width,
                        source: None,
                    };
                    *column += width;
                    Some(atom)
                })
                .collect::<Vec<_>>();
            prefix_atoms.append(&mut rendered_line.atoms);
            rendered_line.atoms = prefix_atoms;
            for span in &mut rendered_line.styled.spans {
                span.start_col += prefix_char_len;
                span.end_col += prefix_char_len;
            }
            if let Some(style) = prefix_style {
                rendered_line.styled.spans.push(Span {
                    start_col: 0,
                    end_col: prefix_char_len,
                    style,
                });
            }
        }
    }

    // ── VW-1: Headings ───────────────────────────────────────────────

    fn render_heading(&mut self, level: u8, inlines: &[Inline], source: &Range<usize>) {
        let heading_style = match level {
            1 => SemanticStyle::Heading1,
            2 => SemanticStyle::Heading2,
            3 => SemanticStyle::Heading3,
            4 => SemanticStyle::Heading4,
            5 => SemanticStyle::Heading5,
            6 => SemanticStyle::Heading6,
            _ => SemanticStyle::Heading1,
        };

        // Build heading prefix glyph
        let prefix: String = match level {
            1 => "█ ".to_string(),
            2 => "▌ ".to_string(),
            3..=6 => {
                let dots = "▪".repeat(level as usize - 2);
                format!("{} ", dots)
            }
            _ => "# ".to_string(),
        };

        // Render inline content
        let styled = self.render_inlines(inlines, heading_style);

        let mut combined = MappedLine::default();
        combined.push_generated(&prefix, heading_style);
        combined.append(styled);

        // Wrap the heading
        let jump_line = self.lines.len();
        let wrapped = wrap_mapped_line(&combined, self.width, 0);
        for line in wrapped {
            self.make_content_line(line, source.clone());
        }

        // Register heading as jump target
        self.jump_targets.push(JumpTarget {
            line: jump_line,
            kind: TargetKind::Heading(level),
        });
    }

    // ── VW-2: Paragraphs ─────────────────────────────────────────────

    fn render_paragraph(&mut self, inlines: &[Inline], source: &Range<usize>) {
        let first_line = self.lines.len();
        let first_link = self.link_index.len();
        let styled = self.render_inlines(inlines, SemanticStyle::Text);
        let wrapped = wrap_mapped_line(&styled, self.width, 0);
        for line in wrapped {
            self.make_content_line(line, source.clone());
        }
        for link_index in first_link..self.link_index.len() {
            self.jump_targets.push(JumpTarget {
                line: first_line,
                kind: TargetKind::Link(link_index),
            });
        }
    }

    // ── VW-3: Emphasis / VW-4: Inline code ───────────────────────────

    fn render_inlines(&mut self, inlines: &[Inline], default_style: SemanticStyle) -> MappedLine {
        let mut line = MappedLine::default();
        for inline in inlines {
            line.append(self.render_inline(inline, default_style));
        }
        line
    }

    /// Convert an inline node to (text, SemanticStyle).
    ///
    /// Per the inline styling rule: nested emphasis resolves to the
    /// innermost semantic. `Strong(Emph(..))` → `Strong`, etc.
    fn render_inline(&mut self, inline: &Inline, default_style: SemanticStyle) -> MappedLine {
        let mut line = MappedLine::default();
        match inline {
            Inline::Text(leaf) => append_leaf(&mut line, leaf, default_style),
            Inline::Code(leaf) => append_leaf(&mut line, leaf, SemanticStyle::CodeSpan),
            Inline::SoftBreak(leaf) | Inline::HardBreak(leaf) => {
                append_leaf(&mut line, leaf, default_style)
            }
            Inline::Emph(inner) => line.append(self.render_inlines(inner, SemanticStyle::Emphasis)),
            Inline::Strong(inner) => line.append(self.render_inlines(inner, SemanticStyle::Strong)),
            Inline::Strike(inner) => {
                line.append(self.render_inlines(inner, SemanticStyle::Strikethrough))
            }
            Inline::Link {
                text: link_text,
                dest,
            } => {
                let marker = self.register_link(dest.clone());
                line.append(self.render_inlines(link_text, SemanticStyle::Link));
                line.push_generated(&format!(" [{}]", marker), SemanticStyle::Link);
            }
            Inline::Image { alt, dest } => {
                let marker = self.register_link(dest.clone());
                line.push_generated("⧉ ", SemanticStyle::Link);
                line.append(self.render_inlines(alt, SemanticStyle::Link));
                line.push_generated(&format!(" [{}]", marker), SemanticStyle::Link);
            }
            Inline::FootnoteRef(leaf) => append_leaf(&mut line, leaf, SemanticStyle::Link),
            Inline::Html(leaf) => append_leaf(&mut line, leaf, SemanticStyle::HtmlRaw),
        }
        line
    }

    // ── VW-5: Fenced code blocks ─────────────────────────────────────

    fn render_code_fence(
        &mut self,
        lang: &Option<String>,
        content_span: &Range<usize>,
        indented: bool,
        source: &Range<usize>,
    ) {
        if indented {
            // Indented code block: same treatment minus language tag
            self.render_fenced_code_body(lang, content_span, source);
            return;
        }

        let first_fence_line = self.lines.len();

        // Fenced code block with gutter
        let gutter = "▏";
        let lang_tag = lang.as_deref().unwrap_or("");

        // Get fence content
        let fence_content = if content_span.start < content_span.end {
            &self.highlighter.text()[content_span.clone()]
        } else {
            ""
        };

        // If we have a highlighter, use it; otherwise render plain
        let highlighted = if let Some(_hl) = self.highlighter_as_ref() {
            syntax::highlight_snippet(lang_tag, fence_content)
        } else {
            fence_content
                .lines()
                .map(|line| StyledLine {
                    text: line.to_string(),
                    spans: Vec::new(),
                })
                .collect()
        };

        // Build gutter lines
        let lang_line_text = format!("{} {}", gutter, lang_tag);
        self.make_synthetic_styled_line(
            StyledLine {
                text: lang_line_text.clone(),
                spans: vec![Span {
                    start_col: 0,
                    end_col: gutter.chars().count(),
                    style: SemanticStyle::Muted,
                }],
            },
            source.start..content_span.start.min(source.end),
        );

        let content_lines = source_line_spans(self.highlighter.text(), content_span);
        for (index, line) in highlighted.iter().enumerate() {
            let line_source = content_lines
                .get(index)
                .cloned()
                .unwrap_or_else(|| content_span.clone());
            let mut mapped = mapped_from_styled(line.clone(), Some(line_source.start));
            mapped.prepend_generated("▏ ", SemanticStyle::Muted);
            self.make_content_line(mapped, line_source);
        }

        // Closing fence
        self.make_synthetic_styled_line(
            StyledLine {
                text: gutter.to_string(),
                spans: vec![Span {
                    start_col: 0,
                    end_col: gutter.chars().count(),
                    style: SemanticStyle::Muted,
                }],
            },
            content_span.end.min(source.end)..source.end,
        );

        for line in &mut self.lines[first_fence_line..] {
            line.role = RenderedLineRole::CodeFence;
        }
    }

    fn render_fenced_code_body(
        &mut self,
        _lang: &Option<String>,
        content_span: &Range<usize>,
        source: &Range<usize>,
    ) {
        let content = if content_span.start < content_span.end {
            &self.highlighter.text()[content_span.clone()]
        } else {
            ""
        };

        let lines: Vec<StyledLine> = content
            .lines()
            .map(|line| StyledLine {
                text: line.to_string(),
                spans: Vec::new(),
            })
            .collect();

        let source_lines = source_line_spans(self.highlighter.text(), content_span);
        for (index, line) in lines.into_iter().enumerate() {
            let source_line = source_lines
                .get(index)
                .cloned()
                .unwrap_or_else(|| source.clone());
            self.make_source_content_line(line, source_line.clone(), source_line.start);
        }
    }

    // ── VW-6: Bulleted lists ─────────────────────────────────────────

    fn render_list(&mut self, ordered: Option<u64>, items: &[crate::rendered::blocks::ListItem]) {
        self.render_list_at_depth(ordered, items, 0);
    }

    fn render_list_at_depth(
        &mut self,
        ordered: Option<u64>,
        items: &[crate::rendered::blocks::ListItem],
        depth: usize,
    ) {
        for (index, item) in items.iter().enumerate() {
            let marker = match item.task {
                Some(true) => "☑".to_string(),
                Some(false) => "☐".to_string(),
                None => match ordered {
                    Some(start) => format!("{}.", start + index as u64),
                    None => match depth {
                        0 => "•".to_string(),
                        1 => "◦".to_string(),
                        _ => "▪".to_string(),
                    },
                },
            };
            self.render_list_item(item, depth, &marker);
        }
    }

    fn render_list_item(
        &mut self,
        item: &crate::rendered::blocks::ListItem,
        depth: usize,
        marker: &str,
    ) {
        let indent = "  ".repeat(depth);
        let first_prefix = format!("{indent}{marker} ");
        let prefix_char_len = first_prefix.chars().count();
        let continuation_prefix = " ".repeat(prefix_char_len);

        let available_width = self.width.saturating_sub(prefix_char_len as u16);
        let mut marker_pending = true;

        // Render children into this builder so document-level metadata is shared.
        for child in &item.children {
            if let BlockKind::List { ordered, items, .. } = &child.kind {
                if marker_pending {
                    let marker_start = indent.chars().count();
                    self.make_generated_content_line(
                        StyledLine {
                            text: format!("{indent}{marker}"),
                            spans: vec![Span {
                                start_col: marker_start,
                                end_col: marker_start + marker.chars().count(),
                                style: SemanticStyle::ListMarker,
                            }],
                        },
                        item.span.clone(),
                    );
                    marker_pending = false;
                }
                self.render_list_at_depth(*ordered, items, depth + 1);
                continue;
            }

            let parent_width = self.width;
            let first_child_line = self.lines.len();
            self.width = available_width;
            self.render_block(child);
            self.width = parent_width;

            for rendered_line in &mut self.lines[first_child_line..] {
                let uses_marker = marker_pending && rendered_line.kind == LineKind::Content;
                let prefix = if uses_marker {
                    marker_pending = false;
                    &first_prefix
                } else {
                    &continuation_prefix
                };
                let prefix_width = text_width(prefix);

                if item.task == Some(true) {
                    for span in &mut rendered_line.styled.spans {
                        if span.style == SemanticStyle::Text {
                            span.style = SemanticStyle::Muted;
                        }
                    }
                }

                rendered_line.styled.text.insert_str(0, prefix);
                for atom in &mut rendered_line.atoms {
                    atom.columns.start += prefix_width;
                    atom.columns.end += prefix_width;
                }
                let mut prefix_atoms = display_groups(prefix)
                    .into_iter()
                    .scan(0, |column, group| {
                        let width = text_width(&group);
                        let atom = crate::style::RenderedSourceAtom {
                            columns: *column..*column + width,
                            source: None,
                        };
                        *column += width;
                        Some(atom)
                    })
                    .collect::<Vec<_>>();
                prefix_atoms.append(&mut rendered_line.atoms);
                rendered_line.atoms = prefix_atoms;
                for span in &mut rendered_line.styled.spans {
                    span.start_col += prefix_char_len;
                    span.end_col += prefix_char_len;
                }

                if uses_marker {
                    let marker_start = indent.chars().count();
                    rendered_line.styled.spans.push(Span {
                        start_col: marker_start,
                        end_col: marker_start + marker.chars().count(),
                        style: SemanticStyle::ListMarker,
                    });
                    rendered_line
                        .styled
                        .spans
                        .sort_by_key(|span| span.start_col);
                }
            }
        }
    }

    // ── VW-7: Task lists ─────────────────────────────────────────────

    // Handled within render_list_item via item.task field.

    // ── VW-8: Blockquotes ────────────────────────────────────────────

    fn render_blockquote(&mut self, children: &[Block], _source: &Range<usize>) {
        let quote_prefix = "┃ ";
        let reduced_width = self
            .width
            .saturating_sub(quote_prefix.chars().count() as u16);

        for child in children {
            self.render_block_into(
                child,
                quote_prefix,
                reduced_width,
                Some(SemanticStyle::Quote),
            );
        }
    }

    // ── VW-9: Tables ─────────────────────────────────────────────────

    fn render_table(
        &mut self,
        alignments: &[TableAlignment],
        header: &[Vec<Inline>],
        rows: &[Vec<Vec<Inline>>],
        source: &Range<usize>,
    ) {
        let row_spans = markdown_table_row_spans(self.highlighter.text(), source, rows.len());
        let table_lines = table::render_table_with_rows(alignments, header, rows, source.clone());

        for (index, line) in table_lines.into_iter().enumerate() {
            let logical_row = line.logical_row;
            let nearest_row = logical_row.unwrap_or(if index == 0 { 0 } else { rows.len() });
            let row_source = row_spans
                .get(nearest_row)
                .cloned()
                .unwrap_or_else(|| source.clone());
            let (styled, atoms) = line.into_parts();
            if logical_row.is_some() {
                self.lines.push(RenderedLine {
                    styled,
                    source: row_source.clone(),
                    kind: LineKind::Content,
                    role: RenderedLineRole::Document,
                    atoms,
                });
                self.set_last_content_source(row_source);
            } else {
                self.lines.push(RenderedLine {
                    styled,
                    source: row_source,
                    kind: LineKind::Synthetic,
                    role: RenderedLineRole::Document,
                    atoms,
                });
            }
        }
    }

    // ── VW-10: Thematic break ────────────────────────────────────────

    fn render_rule(&mut self, source: &Range<usize>) {
        // Full-width rule
        let rule_width = self.width as usize;
        let rule_text = "─".repeat(rule_width);

        self.make_generated_content_line(
            StyledLine {
                text: rule_text,
                spans: vec![Span {
                    start_col: 0,
                    end_col: rule_width,
                    style: SemanticStyle::Rule,
                }],
            },
            source.clone(),
        );
    }

    // ── VW-11: Links ─────────────────────────────────────────────────

    // Handled inline via register_link() in render_inlines.

    // ── VW-12: Images ────────────────────────────────────────────────

    // Handled inline via register_link() in render_inlines.

    // ── VW-13: HTML ──────────────────────────────────────────────────

    fn render_html_block(&mut self, content_span: &Range<usize>, source: &Range<usize>) {
        let content = if content_span.start < content_span.end {
            &self.highlighter.text()[content_span.clone()]
        } else {
            ""
        };

        // Render HTML verbatim in muted monospace style
        let html_lines = source_line_spans(self.highlighter.text(), content_span);
        for (index, line) in content.lines().enumerate() {
            let line_source = html_lines
                .get(index)
                .cloned()
                .unwrap_or_else(|| source.clone());
            self.make_source_content_line(
                StyledLine {
                    text: line.to_string(),
                    spans: vec![Span {
                        start_col: 0,
                        end_col: line.chars().count(),
                        style: SemanticStyle::HtmlRaw,
                    }],
                },
                line_source.clone(),
                line_source.start,
            );
        }
    }

    // ── VW-14: Footnotes ─────────────────────────────────────────────

    fn render_footnote_def(&mut self, label: &str, children: &[Block], source: &Range<usize>) {
        // Store for later appending at document end
        self.footnote_defs.push(FootnoteDef {
            label: label.to_string(),
            children: children.to_vec(),
            source: source.clone(),
        });
    }

    fn append_footnotes(&mut self) {
        if self.footnote_defs.is_empty() {
            return;
        }

        self.add_synthetic_blank(Range { start: 0, end: 0 });

        // Footnotes separator
        let separator = "─ footnotes ─";
        self.make_synthetic_styled_line(
            StyledLine {
                text: separator.to_string(),
                spans: vec![Span {
                    start_col: 0,
                    end_col: separator.chars().count(),
                    style: SemanticStyle::Rule,
                }],
            },
            Range { start: 0, end: 0 },
        );

        let footnote_defs = self.footnote_defs.clone();
        for fd in &footnote_defs {
            // Reference marker
            let marker = format!("[{}]: ", fd.label);
            self.make_generated_content_line(
                StyledLine {
                    text: marker.clone(),
                    spans: vec![Span {
                        start_col: 0,
                        end_col: marker.chars().count(),
                        style: SemanticStyle::Link,
                    }],
                },
                fd.source.clone(),
            );

            // Render footnote body
            for child in &fd.children {
                self.render_block(child);
            }
        }
    }

    fn append_link_index(&mut self) {
        if self.link_index.is_empty() {
            return;
        }

        // Links separator
        let separator = "─ links ─";
        self.make_synthetic_styled_line(
            StyledLine {
                text: separator.to_string(),
                spans: vec![Span {
                    start_col: 0,
                    end_col: separator.chars().count(),
                    style: SemanticStyle::Rule,
                }],
            },
            Range { start: 0, end: 0 },
        );

        let link_index = self.link_index.clone();
        for (marker, dest) in &link_index {
            let line_text = format!("[{}] {}", marker, dest);
            self.make_synthetic_styled_line(
                StyledLine {
                    text: line_text,
                    spans: vec![Span {
                        start_col: 0,
                        end_col: format!("[{}]", marker).chars().count(),
                        style: SemanticStyle::Link,
                    }],
                },
                Range { start: 0, end: 0 },
            );
        }
    }

    // ── FR-3.6: Front matter panel ───────────────────────────────────

    fn render_front_matter(&mut self, block: &Block) {
        let fm_span = &block.span;
        let key_count = match crate::frontmatter::parse_front_matter(self.highlighter.text()) {
            crate::frontmatter::FrontMatter::Yaml(Ok(value))
            | crate::frontmatter::FrontMatter::Toml(Ok(value)) => {
                value.as_map().map_or(0, |map| map.len())
            }
            _ => 0,
        };
        self.render_fm_panel(fm_span, self.front_matter_collapsed, key_count);
    }

    fn render_fm_panel(&mut self, source: &Range<usize>, collapsed: bool, key_count: usize) {
        let physical_lines = source_line_spans(self.highlighter.text(), source);
        let Some(opening_source) = physical_lines.first().cloned() else {
            return;
        };
        let opening_delimiter =
            self.highlighter.text()[opening_source.clone()].trim_end_matches(['\r', '\n']);
        let has_closing_delimiter = physical_lines.last().is_some_and(|line_source| {
            physical_lines.len() > 1
                && self.highlighter.text()[line_source.clone()].trim_end_matches(['\r', '\n'])
                    == opening_delimiter
        });

        if collapsed {
            let mut text = format!("▸ metadata ({key_count} keys)");
            text = text.chars().take(self.width as usize).collect();
            self.make_generated_metadata_line(
                StyledLine {
                    spans: vec![Span {
                        start_col: 0,
                        end_col: text.chars().count(),
                        style: SemanticStyle::FmDelimiter,
                    }],
                    text,
                },
                opening_source,
            );
            return;
        }

        let last_index = physical_lines.len().saturating_sub(1);
        for (index, line_source) in physical_lines.into_iter().enumerate() {
            if index == 0 || (index == last_index && has_closing_delimiter) {
                let text = metadata_border(self.width as usize, index == 0);
                self.make_generated_metadata_line(
                    StyledLine {
                        spans: vec![Span {
                            start_col: 0,
                            end_col: text.chars().count(),
                            style: SemanticStyle::FmDelimiter,
                        }],
                        text,
                    },
                    line_source,
                );
                continue;
            }

            let source_line_number = self.highlighter.text()[..line_source.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let styled = self
                .highlighter
                .highlight_lines(source_line_number..source_line_number + 1)
                .into_iter()
                .next()
                .unwrap_or_else(|| StyledLine {
                    text: self.highlighter.text()[line_source.clone()]
                        .trim_end_matches(['\r', '\n'])
                        .to_string(),
                    spans: Vec::new(),
                });
            self.render_metadata_body_line(styled, line_source);
        }
    }

    fn make_metadata_line(&mut self, mapped: MappedLine, source: Range<usize>) {
        let (styled, atoms) = mapped.into_parts();
        self.lines.push(RenderedLine {
            styled,
            source: source.clone(),
            kind: LineKind::Content,
            role: RenderedLineRole::Metadata,
            atoms,
        });
        self.set_last_content_source(source);
    }

    fn make_generated_metadata_line(&mut self, styled: StyledLine, source: Range<usize>) {
        self.make_metadata_line(mapped_from_styled(styled, None), source);
    }

    fn render_metadata_body_line(&mut self, styled: StyledLine, source: Range<usize>) {
        let width = self.width as usize;
        let mapped = mapped_from_styled(styled, Some(source.start));
        if width < 4 {
            for mut line in wrap_mapped_line(&mapped, self.width, 0) {
                while line.width() > width {
                    line.fragments.pop();
                }
                self.make_metadata_line(line, source.clone());
            }
            return;
        }

        let content_width = self.width.saturating_sub(4).max(1);
        for mut line in wrap_mapped_line(&mapped, content_width, 0) {
            let body_width = line.width();
            let padding = content_width as usize - body_width.min(content_width as usize);
            line.prepend_generated("│ ", SemanticStyle::FmDelimiter);
            line.push_generated(&" ".repeat(padding), SemanticStyle::Text);
            line.push_generated(" │", SemanticStyle::FmDelimiter);
            self.make_metadata_line(line, source.clone());
        }
    }

    fn highlighter_as_ref(&self) -> Option<&syntax::Highlighter> {
        Some(self.highlighter)
    }
}

fn rendered_line_numbers(lines: &[RenderedLine], text: &str) -> Vec<Option<usize>> {
    let mut previous_content_source: Option<Range<usize>> = None;
    lines
        .iter()
        .map(|line| {
            if line.kind != LineKind::Content
                || previous_content_source.as_ref() == Some(&line.source)
            {
                return None;
            }
            previous_content_source = Some(line.source.clone());
            let start = line.source.start.min(text.len());
            Some(text[..start].bytes().filter(|byte| *byte == b'\n').count() + 1)
        })
        .collect()
}

// ── Footnote storage ────────────────────────────────────────────────────────

#[derive(Clone)]
struct FootnoteDef {
    label: String,
    children: Vec<Block>,
    source: Range<usize>,
}

/// Map logical table rows to their exact Markdown source lines, excluding the
/// alignment delimiter line between the header and body.
fn markdown_table_row_spans(
    text: &str,
    source: &Range<usize>,
    body_rows: usize,
) -> Vec<Range<usize>> {
    let Some(table_source) = text.get(source.clone()) else {
        return vec![source.clone()];
    };
    let mut physical = Vec::new();
    let mut start = source.start;
    for line in table_source.split_inclusive('\n') {
        let end = start + line.len();
        physical.push(start..end);
        start = end;
    }
    if start < source.end {
        physical.push(start..source.end);
    }

    let mut logical = Vec::with_capacity(body_rows + 1);
    if let Some(header) = physical.first() {
        logical.push(header.clone());
    }
    logical.extend(physical.into_iter().skip(2).take(body_rows));
    logical
}

fn source_line_spans(text: &str, source: &Range<usize>) -> Vec<Range<usize>> {
    let Some(source_text) = text.get(source.clone()) else {
        return vec![source.clone()];
    };
    let mut spans = Vec::new();
    let mut start = source.start;
    for line in source_text.split_inclusive('\n') {
        let end = start + line.len();
        spans.push(start..end);
        start = end;
    }
    if start < source.end {
        spans.push(start..source.end);
    }
    spans
}

fn append_leaf(line: &mut MappedLine, leaf: &InlineLeaf, style: SemanticStyle) {
    for atom in &leaf.atoms {
        line.push(atom.text.clone(), style, Some(atom.source.clone()));
    }
}

fn mapped_from_styled(styled: StyledLine, visible_start: Option<usize>) -> MappedLine {
    let mut mapped = MappedLine::default();
    let mut char_offset = 0;
    let mut byte_offset = 0;
    for group in display_groups(&styled.text) {
        let char_len = group.chars().count();
        let style = styled
            .spans
            .iter()
            .rev()
            .find(|span| span.start_col <= char_offset && char_offset < span.end_col)
            .map_or(SemanticStyle::Text, |span| span.style);
        let source =
            visible_start.map(|start| start + byte_offset..start + byte_offset + group.len());
        byte_offset += group.len();
        char_offset += char_len;
        mapped.push(group, style, source);
    }
    mapped
}

fn display_groups(text: &str) -> Vec<String> {
    let mut groups: Vec<String> = Vec::new();
    for character in text.chars() {
        if character.width().unwrap_or(0) == 0 {
            if let Some(previous) = groups.last_mut() {
                previous.push(character);
            } else {
                groups.push(character.to_string());
            }
        } else {
            groups.push(character.to_string());
        }
    }
    groups
}

fn metadata_border(width: usize, opening: bool) -> String {
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return if opening { "┌" } else { "└" }.to_string();
    }
    let (left, right) = if opening {
        ('┌', '┐')
    } else {
        ('└', '┘')
    };
    if !opening || width < 12 {
        return format!("{left}{}{right}", "─".repeat(width - 2));
    }
    let label = "─ metadata ";
    format!(
        "{left}{label}{}{right}",
        "─".repeat(width.saturating_sub(label.chars().count() + 2))
    )
}

// ── Re-exports ─────────────────────────────────────────────────────────────

pub(crate) use blocks::*;
pub(crate) use wrap::{text_width, wrap_source_line};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_text_inherits_its_level_style() {
        let text = "# one\n\n## two\n\n### three\n\n#### four\n\n##### five\n\n###### six\n";
        let model = BlockModel::build(text, None);
        let highlighter = syntax::Highlighter::new(text);
        let layout = RenderedLayout::build(&model, 80, &highlighter);

        for (label, expected_style) in [
            ("one", SemanticStyle::Heading1),
            ("two", SemanticStyle::Heading2),
            ("three", SemanticStyle::Heading3),
            ("four", SemanticStyle::Heading4),
            ("five", SemanticStyle::Heading5),
            ("six", SemanticStyle::Heading6),
        ] {
            let line = layout
                .lines
                .iter()
                .find(|line| line.styled.text.contains(label))
                .unwrap_or_else(|| panic!("missing rendered heading {label:?}"));
            assert!(
                line.styled.spans.iter().any(|span| {
                    span.style == expected_style && span_text(&line.styled, span).contains(label)
                }),
                "heading text {label:?} must use {expected_style:?}: {:?}",
                line.styled.spans
            );
        }
    }

    #[test]
    fn heading_inline_styles_override_the_heading_level() {
        let text = "## plain *emphasis* `code`\n";
        let model = BlockModel::build(text, None);
        let highlighter = syntax::Highlighter::new(text);
        let layout = RenderedLayout::build(&model, 80, &highlighter);
        let heading = layout
            .lines
            .iter()
            .find(|line| line.styled.text.contains("plain"))
            .expect("rendered heading");

        for (content, expected_style) in [
            ("plain ", SemanticStyle::Heading2),
            ("emphasis", SemanticStyle::Emphasis),
            ("code", SemanticStyle::CodeSpan),
        ] {
            assert!(
                heading.styled.spans.iter().any(|span| {
                    span.style == expected_style
                        && span_text(&heading.styled, span).contains(content)
                }),
                "heading content {content:?} must use {expected_style:?}: {:?}",
                heading.styled.spans
            );
        }
    }

    #[test]
    fn unicode_front_matter_panel_spans_select_key_and_value() {
        let text = "---\ntítulo: café\n---\n";
        let layout = front_matter_layout(text);
        let row = layout
            .lines
            .iter()
            .find(|line| line.styled.text.contains("título"))
            .expect("front-matter row should be rendered");
        let key_span = row
            .styled
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::FmKey)
            .expect("front-matter key should be styled");
        let value_span = row
            .styled
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::FmValue)
            .expect("front-matter value should be styled");

        assert_eq!(span_text(&row.styled, key_span), "título");
        assert_eq!(span_text(&row.styled, value_span), "café");
        assert!(value_span.end_col <= row.styled.text.chars().count());
    }

    #[test]
    fn front_matter_border_does_not_shift_ascii_value_span() {
        let text = "---\nkey: value\n---\n";
        let layout = front_matter_layout(text);
        let row = layout
            .lines
            .iter()
            .find(|line| line.styled.text.contains("key"))
            .expect("front-matter row should be rendered");
        let value_span = row
            .styled
            .spans
            .iter()
            .find(|span| span.style == SemanticStyle::FmValue)
            .expect("front-matter value should be styled");

        assert!(row.styled.text.starts_with('│'));
        assert_eq!(span_text(&row.styled, value_span), "value");
    }

    #[test]
    fn unterminated_front_matter_keeps_the_final_metadata_line() {
        for text in ["---\ntitle: café", "+++\ntitle = \"café\""] {
            let layout = front_matter_layout(text);
            let row = layout
                .lines
                .iter()
                .find(|line| line.styled.text.contains("title"))
                .expect("unterminated final metadata line should be rendered");
            assert_eq!(row.role, RenderedLineRole::Metadata);
            assert_eq!(row.source, text.find("title").unwrap()..text.len());
            let owned_source = row
                .atoms
                .iter()
                .filter_map(|atom| atom.source.as_ref())
                .map(|source| &text[source.clone()])
                .collect::<String>();
            assert!(owned_source.contains("title"));
            assert!(!layout
                .lines
                .iter()
                .any(|line| line.styled.text.starts_with('└')));
        }
    }

    #[test]
    fn named_and_numeric_entities_own_their_complete_source() {
        for (text, entity) in [
            ("A &amp; B\n", "&amp;"),
            ("A &#38; B\n", "&#38;"),
            ("A &#x26; B\n", "&#x26;"),
            ("A &fjlig; B\n", "&fjlig;"),
        ] {
            let model = BlockModel::build(text, None);
            let highlighter = syntax::Highlighter::new(text);
            let layout = RenderedLayout::build(&model, 80, &highlighter);
            let entity_start = text.find(entity).unwrap();
            assert!(layout
                .lines
                .iter()
                .flat_map(|line| &line.atoms)
                .any(|atom| atom.source == Some(entity_start..entity_start + entity.len())));
        }
    }

    fn front_matter_layout(text: &str) -> RenderedLayout {
        let model = BlockModel::build(text, Some(0..text.len()));
        let highlighter = syntax::Highlighter::new(text);
        RenderedLayout::build(&model, 80, &highlighter)
    }

    fn span_text(line: &StyledLine, span: &Span) -> String {
        line.text
            .chars()
            .skip(span.start_col)
            .take(span.end_col - span.start_col)
            .collect()
    }
}

#[cfg(test)]
#[path = "tests/blocks.rs"]
mod block_contract_tests;

#[cfg(test)]
#[path = "tests/goldens.rs"]
mod golden_contract_tests;
