//! View layout renderer — produces styled, wrapped, source-mapped rendered lines.
//!
//! `ViewLayout::build(&BlockModel, width, &Highlighter)` implements every VW
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

use crate::style::{
    JumpTarget, LineKind, SemanticStyle, Span, StyledLine, TargetKind, ViewLayout, ViewLine,
};
use crate::syntax;

// ── Public API ─────────────────────────────────────────────────────────────

impl ViewLayout {
    /// Build a `ViewLayout` from a `BlockModel`, layout width, and highlighter.
    ///
    /// Produces styled, wrapped, source-mapped rendered lines implementing
    /// every VW row (VW-1..VW-14) and the front-matter panel (FR-3.6).
    ///
    /// Blank-line policy: exactly one synthetic blank line between top-level
    /// blocks; no leading blank before the first line; no trailing blank after
    /// the last.
    pub fn build(model: &BlockModel, width: u16, highlighter: &syntax::Highlighter) -> Self {
        if width == 0 {
            return Self::default();
        }

        let layout = ViewLayoutBuilder::new(model, width, highlighter);
        layout.build()
    }
}

// ── Builder ────────────────────────────────────────────────────────────────

struct ViewLayoutBuilder<'a> {
    model: &'a BlockModel,
    width: u16,
    highlighter: &'a syntax::Highlighter,
    lines: Vec<ViewLine>,
    jump_targets: Vec<JumpTarget>,
    link_index: Vec<(usize, String)>,
    last_content_source: Option<Range<usize>>,
    next_link_marker: usize,
    footnote_defs: Vec<FootnoteDef>,
}

impl<'a> ViewLayoutBuilder<'a> {
    fn new(model: &'a BlockModel, width: u16, highlighter: &'a syntax::Highlighter) -> Self {
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
        }
    }

    fn build(mut self) -> ViewLayout {
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

        ViewLayout {
            lines: self.lines,
            jump_targets: self.jump_targets,
            link_index: self.link_index,
        }
    }

    fn add_synthetic_blank(&mut self, source: Range<usize>) {
        let source = self.last_content_source.clone().unwrap_or(source);
        self.lines.push(ViewLine {
            styled: StyledLine {
                text: String::new(),
                spans: Vec::new(),
            },
            source: source.clone(),
            kind: LineKind::Synthetic,
        });
    }

    fn set_last_content_source(&mut self, source: Range<usize>) {
        self.last_content_source = Some(source);
    }

    fn make_content_line(&mut self, styled: StyledLine, source: Range<usize>) {
        self.lines.push(ViewLine {
            styled,
            source: source.clone(),
            kind: LineKind::Content,
        });
        self.set_last_content_source(source);
    }

    fn make_synthetic_line(&mut self, styled: StyledLine, source: Range<usize>) {
        let source = self.last_content_source.clone().unwrap_or(source);
        self.lines.push(ViewLine {
            styled,
            source,
            kind: LineKind::Synthetic,
        });
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
            } => self.render_list(*ordered, items, &block.span),
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
        for view_line in &mut self.lines[first_child_line..] {
            view_line.styled.text.insert_str(0, prefix);
            for span in &mut view_line.styled.spans {
                span.start_col += prefix_char_len;
                span.end_col += prefix_char_len;
            }
            if let Some(style) = prefix_style {
                view_line.styled.spans.push(Span {
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

        // Combine prefix + content
        let mut combined_text = prefix.to_string();
        combined_text.push_str(&styled.text);

        let mut combined_spans = styled.spans;
        // Adjust span offsets by prefix length (in chars, not bytes)
        let prefix_char_len = prefix.chars().count();
        for span in &mut combined_spans {
            span.start_col += prefix_char_len;
            span.end_col += prefix_char_len;
        }

        // Apply heading style to prefix
        combined_spans.push(Span {
            start_col: 0,
            end_col: prefix_char_len,
            style: heading_style,
        });
        combined_spans.sort_by_key(|s| s.start_col);

        let combined = StyledLine {
            text: combined_text,
            spans: combined_spans,
        };

        // Wrap the heading
        let jump_line = self.lines.len();
        let wrapped = wrap_lines(&combined, self.width, 0);
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
        let styled = self.render_inlines(inlines, SemanticStyle::Text);
        let wrapped = wrap_lines(&styled, self.width, 0);
        for line in wrapped {
            self.make_content_line(line, source.clone());
        }
    }

    // ── VW-3: Emphasis / VW-4: Inline code ───────────────────────────

    fn render_inlines(&mut self, inlines: &[Inline], default_style: SemanticStyle) -> StyledLine {
        let mut text = String::new();
        let mut spans = Vec::new();
        let mut col_offset: usize = 0;

        for inline in inlines {
            let (inline_text, style) = self.inline_to_text_and_style(inline);
            let style = if style == SemanticStyle::Text {
                default_style
            } else {
                style
            };
            let char_len = inline_text.chars().count();

            text.push_str(&inline_text);

            if style != default_style || !inline_text.is_empty() {
                spans.push(Span {
                    start_col: col_offset,
                    end_col: col_offset + char_len,
                    style,
                });
            }

            col_offset += char_len;
        }

        StyledLine { text, spans }
    }

    /// Convert an inline node to (text, SemanticStyle).
    ///
    /// Per the inline styling rule: nested emphasis resolves to the
    /// innermost semantic. `Strong(Emph(..))` → `Strong`, etc.
    fn inline_to_text_and_style(&mut self, inline: &Inline) -> (String, SemanticStyle) {
        match inline {
            Inline::Text(t) => (t.clone(), SemanticStyle::Text),
            Inline::Code(c) => (c.clone(), SemanticStyle::CodeSpan),
            Inline::SoftBreak => (" ".to_string(), SemanticStyle::Text),
            Inline::HardBreak => ("  ".to_string(), SemanticStyle::Text),
            Inline::Emph(inner) => {
                let styled = self.render_inlines(inner, SemanticStyle::Emphasis);
                (styled.text, SemanticStyle::Emphasis)
            }
            Inline::Strong(inner) => {
                let styled = self.render_inlines(inner, SemanticStyle::Strong);
                (styled.text, SemanticStyle::Strong)
            }
            Inline::Strike(inner) => {
                let styled = self.render_inlines(inner, SemanticStyle::Strikethrough);
                (styled.text, SemanticStyle::Strikethrough)
            }
            Inline::Link {
                text: link_text,
                dest,
            } => {
                let marker = self.register_link(dest.clone());
                let text = self.render_inlines(link_text, SemanticStyle::Link).text;
                let marker_text = format!("[{}]", marker);
                (format!("{} {}", text, marker_text), SemanticStyle::Link)
            }
            Inline::Image { alt, dest } => {
                let marker = self.register_link(dest.clone());
                let marker_text = format!("⧉ {} [{}]", alt, marker);
                (marker_text, SemanticStyle::Link)
            }
            Inline::FootnoteRef(label) => (format!("[{}]", label), SemanticStyle::Link),
            Inline::Html(h) => (h.clone(), SemanticStyle::HtmlRaw),
        }
    }

    // ── VW-5: Fenced code blocks ─────────────────────────────────────

    fn render_code_fence(
        &mut self,
        lang: &Option<String>,
        content_span: &Range<usize>,
        indented: bool,
        source: &Range<usize>,
    ) {
        let source_range = source.clone();

        if indented {
            // Indented code block: same treatment minus language tag
            self.render_fenced_code_body(lang, content_span, &source_range);
            return;
        }

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
        self.make_content_line(
            StyledLine {
                text: lang_line_text.clone(),
                spans: vec![Span {
                    start_col: 0,
                    end_col: gutter.chars().count(),
                    style: SemanticStyle::Muted,
                }],
            },
            source_range.clone(),
        );

        for line in &highlighted {
            let text = format!("{} {}", gutter, line.text);
            let mut spans = line.spans.clone();
            // Shift spans by gutter width
            let gutter_width = gutter.chars().count() + 1; // gutter + space
            for span in &mut spans {
                span.start_col += gutter_width;
                span.end_col += gutter_width;
            }
            // Add gutter style
            spans.push(Span {
                start_col: 0,
                end_col: gutter.chars().count(),
                style: SemanticStyle::Muted,
            });
            self.make_content_line(StyledLine { text, spans }, source_range.clone());
        }

        // Closing fence
        self.make_content_line(
            StyledLine {
                text: gutter.to_string(),
                spans: vec![Span {
                    start_col: 0,
                    end_col: gutter.chars().count(),
                    style: SemanticStyle::Muted,
                }],
            },
            source_range,
        );
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

        for line in lines {
            self.make_content_line(line, source.clone());
        }
    }

    // ── VW-6: Bulleted lists ─────────────────────────────────────────

    fn render_list(
        &mut self,
        ordered: Option<u64>,
        items: &[crate::view::blocks::ListItem],
        source: &Range<usize>,
    ) {
        for item in items {
            self.render_list_item(item, 0, ordered, source);
        }
    }

    fn render_list_item(
        &mut self,
        item: &crate::view::blocks::ListItem,
        depth: usize,
        _ordered: Option<u64>,
        _source: &Range<usize>,
    ) {
        // Determine bullet glyph
        let bullet = match item.task {
            Some(true) => "☑",
            Some(false) => "☐",
            None => match depth {
                0 => "•",
                1 => "◦",
                _ => "▪",
            },
        };

        let indent = "  ".repeat(depth);
        let prefix = format!("{}{} ", indent, bullet);
        let prefix_char_len = prefix.chars().count();

        let available_width = self.width.saturating_sub(prefix_char_len as u16);

        // Render children into this builder so document-level metadata is shared.
        for child in &item.children {
            self.render_block_into(child, &prefix, available_width, None);
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
        let table_lines = table::render_table(alignments, header, rows, source.clone());

        for line in table_lines {
            self.make_content_line(line, source.clone());
        }
    }

    // ── VW-10: Thematic break ────────────────────────────────────────

    fn render_rule(&mut self, source: &Range<usize>) {
        // Full-width rule
        let rule_width = self.width as usize;
        let rule_text = "─".repeat(rule_width);

        self.make_content_line(
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
        for line in content.lines() {
            self.make_content_line(
                StyledLine {
                    text: line.to_string(),
                    spans: vec![Span {
                        start_col: 0,
                        end_col: line.chars().count(),
                        style: SemanticStyle::HtmlRaw,
                    }],
                },
                source.clone(),
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
        self.make_synthetic_line(
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
            self.make_content_line(
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
        self.make_synthetic_line(
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
            self.make_synthetic_line(
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
        // We need to check the document's front matter to render the panel
        // For now, render a placeholder — the full implementation reads
        // FrontMatter from the document context.
        //
        // Since ViewLayout::build only takes BlockModel + width + Highlighter,
        // we render front matter as a synthetic block.
        let fm_span = &block.span;

        // Try to get front matter from the highlighter's document text
        // The highlighter has the full text; we parse front matter from it
        let text = self.highlighter.text();
        let fm = crate::frontmatter::parse_front_matter(text);

        if matches!(fm, crate::frontmatter::FrontMatter::None) {
            return;
        }
        if let crate::frontmatter::FrontMatter::Yaml(Ok(value))
        | crate::frontmatter::FrontMatter::Toml(Ok(value)) = fm
        {
            self.render_fm_panel(&value, fm_span, false);
        } else if let crate::frontmatter::FrontMatter::Yaml(Err(_))
        | crate::frontmatter::FrontMatter::Toml(Err(_)) = fm
        {
            self.render_fm_error(fm_span);
        }
    }

    fn render_fm_panel(
        &mut self,
        value: &crate::frontmatter::Value,
        source: &Range<usize>,
        collapsed: bool,
    ) {
        if let Some(map) = value.as_map() {
            let key_count = map.len();

            if collapsed {
                // Collapsed form: single line
                let meta_type = "metadata";
                let line_text = format!("▸ {} ({} keys)", meta_type, key_count);
                self.make_content_line(
                    StyledLine {
                        text: line_text,
                        spans: Vec::new(),
                    },
                    source.clone(),
                );
                return;
            }

            // Expanded form: bordered box with key: value rows
            let border_top = "┌─ metadata ─┐";
            self.make_content_line(
                StyledLine {
                    text: border_top.to_string(),
                    spans: Vec::new(),
                },
                source.clone(),
            );

            for (key, val) in map {
                let val_str = fm_value_to_compact(val);
                let line_text = format!("│ {}: {} │", key, val_str);
                let line_char_count = line_text.chars().count();
                let colon_pos = line_text
                    .chars()
                    .enumerate()
                    .filter_map(|(position, ch)| (ch == ':').then_some(position + 2))
                    .last()
                    .unwrap_or(line_char_count);
                self.make_content_line(
                    StyledLine {
                        text: line_text.clone(),
                        spans: vec![
                            Span {
                                start_col: 2,
                                end_col: 2 + key.chars().count(),
                                style: SemanticStyle::FmKey,
                            },
                            Span {
                                start_col: colon_pos,
                                end_col: line_char_count.saturating_sub(2),
                                style: SemanticStyle::FmValue,
                            },
                        ],
                    },
                    source.clone(),
                );
            }

            let border_bottom = "└──────────────┘";
            self.make_content_line(
                StyledLine {
                    text: border_bottom.to_string(),
                    spans: Vec::new(),
                },
                source.clone(),
            );
        }
    }

    fn render_fm_error(&mut self, source: &Range<usize>) {
        let text = "⚠ front matter unparsed";
        self.make_content_line(
            StyledLine {
                text: text.to_string(),
                spans: vec![Span {
                    start_col: 0,
                    end_col: text.chars().count(),
                    style: SemanticStyle::Muted,
                }],
            },
            source.clone(),
        );
    }

    fn highlighter_as_ref(&self) -> Option<&syntax::Highlighter> {
        Some(self.highlighter)
    }
}

// ── Footnote storage ────────────────────────────────────────────────────────

#[derive(Clone)]
struct FootnoteDef {
    label: String,
    children: Vec<Block>,
    source: Range<usize>,
}

// ── Front matter value → compact string ────────────────────────────────────

fn fm_value_to_compact(value: &crate::frontmatter::Value) -> String {
    match value {
        crate::frontmatter::Value::Str(s) => format!("\"{}\"", s),
        crate::frontmatter::Value::Num(n) => match n {
            crate::frontmatter::Num::Int(i) => i.to_string(),
            crate::frontmatter::Num::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    format!("{}.0", f)
                } else {
                    f.to_string()
                }
            }
        },
        crate::frontmatter::Value::Bool(b) => b.to_string(),
        crate::frontmatter::Value::Seq(items) => {
            let inner: Vec<String> = items.iter().map(fm_value_to_compact).collect();
            format!("[{}]", inner.join(", "))
        }
        crate::frontmatter::Value::Map(map) => {
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}: {}", k, fm_value_to_compact(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
    }
}

// ── Re-exports ─────────────────────────────────────────────────────────────

pub use blocks::*;
pub use table::render_table;
pub use wrap::{text_width, wrap_lines};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_text_inherits_its_level_style() {
        let text = "# one\n\n## two\n\n### three\n\n#### four\n\n##### five\n\n###### six\n";
        let model = BlockModel::build(text, None);
        let highlighter = syntax::Highlighter::new(text);
        let layout = ViewLayout::build(&model, 80, &highlighter);

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
        let layout = ViewLayout::build(&model, 80, &highlighter);
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
        assert_eq!(span_text(&row.styled, value_span), "\"café\"");
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
        assert_eq!(span_text(&row.styled, value_span), "\"value\"");
    }

    fn front_matter_layout(text: &str) -> ViewLayout {
        let model = BlockModel::build(text, Some(0..text.len()));
        let highlighter = syntax::Highlighter::new(text);
        ViewLayout::build(&model, 80, &highlighter)
    }

    fn span_text(line: &StyledLine, span: &Span) -> String {
        line.text
            .chars()
            .skip(span.start_col)
            .take(span.end_col - span.start_col)
            .collect()
    }
}
