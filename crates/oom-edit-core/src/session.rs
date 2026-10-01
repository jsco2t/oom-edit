//! EditorSession — the core editing façade.
//!
//! This module owns the `EditorSession` type, the `Mode` machine, and the
//! session-facing type definitions (`KeyInput`, `KeyCode`, `Modifiers`,
//! `Effect`, `Viewport`). It composes `VimCore` (the hjkl wrapper) with the
//! document model and highlighting pipeline.

mod live_document;

// ── Mode ───────────────────────────────────────────────────────────────────

/// The four user-visible editor modes.
///
/// Normal and Select are rendered Markdown surfaces owned by this session.
/// Insert uses the private Vim wrapper for raw-source editing, and Command
/// owns ex-command entry. Private hjkl modal states never escape
/// `vim.rs`.
///
/// See plan §6.1 / FR-1.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Rendered Normal mode — navigation and editing transitions.
    Normal,
    /// Raw-source Insert mode — direct text entry.
    Insert,
    /// Rendered character-, line-, or block-wise Select mode.
    Select,
    /// Command mode — ex-command entry (e.g. `:w`).
    Command,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{AtomicSaveOperations, FileSystemAtomicSave};
    use std::fs;
    use std::io;
    use std::path::Path;

    struct FaultSave {
        delegate: FileSystemAtomicSave,
        fail_parent_sync: bool,
    }

    impl AtomicSaveOperations for FaultSave {
        type TempFile = tempfile::NamedTempFile;
        type ParentDirectory = fs::File;

        fn create_temp(&mut self, parent: &Path) -> io::Result<Self::TempFile> {
            self.delegate.create_temp(parent)
        }
        fn write_all(&mut self, temp: &mut Self::TempFile, contents: &[u8]) -> io::Result<()> {
            self.delegate.write_all(temp, contents)
        }
        fn set_permissions(
            &mut self,
            temp: &Self::TempFile,
            permissions: fs::Permissions,
        ) -> io::Result<()> {
            self.delegate.set_permissions(temp, permissions)
        }
        fn sync_file(&mut self, temp: &Self::TempFile) -> io::Result<()> {
            if self.fail_parent_sync {
                self.delegate.sync_file(temp)
            } else {
                Err(io::Error::other("injected pre-commit failure"))
            }
        }
        fn persist(&mut self, temp: Self::TempFile, target: &Path) -> io::Result<()> {
            self.delegate.persist(temp, target)
        }
        fn open_parent_read_only(&mut self, parent: &Path) -> io::Result<Self::ParentDirectory> {
            self.delegate.open_parent_read_only(parent)
        }
        fn sync_parent(&mut self, parent: &Self::ParentDirectory) -> io::Result<()> {
            if self.fail_parent_sync {
                Err(io::Error::other("injected post-commit failure"))
            } else {
                self.delegate.sync_parent(parent)
            }
        }
    }

    fn key(c: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(c),
            },
            mods: Modifiers::default(),
        }
    }

    fn esc() -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Esc,
            },
            mods: Modifiers::default(),
        }
    }

    fn special(kind: KeyCodeKind) -> KeyInput {
        KeyInput {
            code: KeyCode { kind },
            mods: Modifiers::default(),
        }
    }

    fn ctrl(c: char) -> KeyInput {
        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(c),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        }
    }

    fn assert_complete_reference(session: &mut EditorSession, width: u16) {
        let text = session.document();
        let fresh_highlighter = crate::syntax::Highlighter::new(&text);
        let fresh_model = BlockModel::build(&text, crate::frontmatter::front_matter_span(&text));
        let (expected, expected_fences) = RenderedLayout::build_with_fence_regions(
            &fresh_model,
            width,
            &fresh_highlighter,
            session.rendered_state.fm_collapsed,
        );
        let actual = session.render_layout(width).clone();
        assert_eq!(
            actual, expected,
            "rendered cells, styles, atoms and indices drifted"
        );
        if let SessionMode::Select(active) = &session.session_mode {
            let mut expected_selection = nav::project_selection_from_source_positions(
                active.anchor.point,
                active.active.point,
                active.kind.shape(),
                active.anchor.source,
                active.active.source,
                &expected,
                &text,
            );
            if let SelectionKind::Character { ranges } = &active.kind {
                expected_selection.source_ranges = ranges.clone();
                expected_selection.rows = nav::character_selection_rows(ranges, &expected);
            }
            assert_eq!(session.rendered_selection(), Some(expected_selection));
        }
        assert_eq!(session.rendered_state.code_fence_regions, expected_fences);
        let work = &session.rendered_state.last_work;
        assert_eq!(
            work.rebuilt_blocks + work.reused_blocks,
            fresh_model.blocks.len()
        );
        assert_eq!(work.rebuilt_rows, actual.lines.len());
        assert_eq!(work.reused_rows, 0);
        let visible_lines = session.line_count().min(48);
        assert_eq!(
            session.live.highlighter().highlight_lines(0..visible_lines),
            fresh_highlighter.highlight_lines(0..visible_lines),
            "source highlighting drifted"
        );
        assert_eq!(
            session.live.front_matter(),
            &crate::frontmatter::parse_front_matter(&text)
        );
        let (
            parse_count,
            changed_ranges,
            changed_bytes,
            injection_count,
            injection_scans,
            reference_scans,
        ) = session.live.highlighter().work_snapshot();
        assert!(parse_count >= 1);
        assert!(changed_bytes <= text.len());
        assert!(changed_ranges <= session.line_count());
        assert!(injection_count <= session.line_count());
        assert!(injection_scans >= 1);
        assert!(reference_scans >= 1);
        assert!(session.rendered_cursor().row < actual.lines.len().max(1));
    }

    #[test]
    fn complete_reference_covers_current_text_after_local_and_global_edits() {
        let documents = [
            "---\r\ntitle: Café\r\n---\r\n\r\n# Heading &amp; \\*literal\\*\r\n\r\n[ref]: https://example.invalid\r\n\r\nA [link][ref] and duplicate duplicate.\r\n\r\n| A | B |\r\n|---|---|\r\n| α | `β` |\r\n\r\n```rust\r\nlet value = 42;\r\n```\r\n\r\n[^note]: footnote\r\n",
            "+++\ntitle = 'Demo'\n+++\n\n> - nested *item*\n> - another item\n\n```unknown\nplain code\n```\n\nA [link](https://example.invalid) and &amp;.\n",
        ];
        for initial in documents {
            let mut session = EditorSession::from_text(initial);
            let baseline = session.document();
            for width in [22, 80] {
                assert_complete_reference(&mut session, width);
            }
            let body_offset = baseline
                .find("Heading")
                .or_else(|| baseline.find("nested"))
                .unwrap();
            session.jump_to_offset(body_offset).unwrap();
            session.render_layout(80);
            session.handle_key(key('V'));
            assert_eq!(session.mode(), Mode::Select);
            for width in [22, 80] {
                assert_complete_reference(&mut session, width);
            }
            session.handle_key(key('d'));
            assert_ne!(session.document(), baseline);
            for width in [22, 80] {
                assert_complete_reference(&mut session, width);
            }
            session.handle_key(key('u'));
            assert_eq!(session.document(), baseline);
            session.handle_key(key('i'));
            session.handle_key(key('x'));
            for width in [22, 80] {
                assert_complete_reference(&mut session, width);
            }
            session.handle_key(esc());
            let text = session.document();
            let source_anchor = session.live.cursor();
            let line_starts = session.live.highlighter().line_starts().to_vec();
            let layout = session.render_layout(80).clone();
            let expected_cursor = nav::enter_rendered_indexed(
                source_anchor.0,
                source_anchor.1,
                &layout,
                &text,
                &line_starts,
            );
            assert_eq!(session.rendered_cursor(), expected_cursor.point());
            session.handle_key(key('u'));
            assert_eq!(session.document(), baseline);
            for width in [22, 80] {
                assert_complete_reference(&mut session, width);
            }
        }
    }

    #[test]
    fn session_publishes_one_local_edit_into_retained_rows_before_full_materialization() {
        let text = "# Heading\n\nA [link](https://example.invalid) and a body line.\nSecond line.\n\nTail.\n";
        let mut session = EditorSession::from_text(text);
        session.render_layout(24);
        session
            .jump_to_offset(text.find("body line").unwrap())
            .unwrap();
        session.ensure_rendered_rows(24);
        assert!(session.rendered_state.layout_cache.is_none());
        assert!(session.rendered_state.row_cache.is_some());

        session.handle_key(key('i'));
        session.handle_key(key('x'));
        session.handle_key(esc());
        assert!(session.rendered_state.row_cache.is_some());
        assert!(session.rendered_state.layout_cache.is_none());
        let viewport = session.rendered_viewport(24, 0, 12);
        let current = session.document();
        let fresh_model =
            BlockModel::build(&current, crate::frontmatter::front_matter_span(&current));
        let fresh_highlighter = crate::syntax::Highlighter::new(&current);
        let expected =
            RenderedLayout::build_with_fence_regions(&fresh_model, 24, &fresh_highlighter, false).0;
        assert_eq!(viewport.first_row, 0);
        assert_eq!(viewport.total_rows, expected.lines.len());
        assert_eq!(viewport.lines, expected.lines[..viewport.lines.len()]);
        assert_eq!(
            viewport.line_numbers,
            expected.line_numbers[..viewport.line_numbers.len()]
        );
        assert_complete_reference(&mut session, 24);
    }

    #[test]
    fn rendered_line_delete_and_undo_keep_indexed_rows_current() {
        let text = crate::realistic_fixtures::generate("mixed", 1024 * 1024);
        let mut session = EditorSession::from_text(&text);
        let source = session.live.highlighter().line_starts()[6];
        session.jump_to_offset(source).unwrap();
        session.rendered_viewport(100, 0, 41);
        session.ensure_rendered_rows(100);
        assert!(session.rendered_state.row_cache.is_some());
        session.handle_key(key('V'));
        assert!(session.rendered_state.row_cache.is_some());
        session.handle_key(key('d'));
        assert!(session.rendered_state.row_cache.is_some());
        let (rebuilt, _, retained) = session.rendered_state.row_cache.as_ref().unwrap().work();
        assert!(rebuilt < 500, "local delete rebuilt {rebuilt} rows");
        assert!(retained > 10_000, "local delete retained {retained} rows");
        let current = session.document();
        let fresh = EditorSession::from_text(&current)
            .render_layout(100)
            .clone();
        for top in [
            0,
            fresh.lines.len() / 2,
            fresh.lines.len().saturating_sub(41),
        ] {
            let viewport = session.rendered_viewport(100, top, 41);
            let first = viewport.first_row;
            let end = first + viewport.lines.len();
            assert_eq!(viewport.lines, fresh.lines[first..end]);
            assert_eq!(viewport.line_numbers, fresh.line_numbers[first..end]);
        }
        let (_, reads, _) = session.rendered_state.row_cache.as_ref().unwrap().work();
        assert!(reads < 150, "three viewports read {reads} rows");
        session.handle_key(key('u'));
        assert!(session.rendered_state.row_cache.is_some());
        assert_eq!(session.document(), text);
        let viewport = session.rendered_viewport(100, 0, 41);
        let restored = EditorSession::from_text(&text).render_layout(100).clone();
        assert_eq!(viewport.lines, restored.lines[..viewport.lines.len()]);
        assert_eq!(
            viewport.line_numbers,
            restored.line_numbers[..viewport.line_numbers.len()]
        );
    }

    #[test]
    fn indexed_viewports_match_complete_rows_and_gutters_at_every_scroll_offset() {
        let documents = [
            "# Café\r\n\r\nA repeated repeated [link](https://example.invalid) and é🙂.\r\n\r\nTail.\r\n",
            "---\ntitle: Note\n---\n\nA [^one] reference and [link](https://example.invalid).\n\n[^one]: footnote body.\n",
            "| A | B |\n|---|---|\n| `x` | α |\n\n```rust\nfn main() {}\n```\n",
        ];
        for text in documents {
            for width in [12, 40] {
                let mut session = EditorSession::from_text(text);
                let expected = session.render_layout(width).clone();
                session.ensure_rendered_rows(width);
                let mut expected_continuations = Vec::new();
                let mut source_line = None;
                for (row, line) in expected.lines.iter().enumerate() {
                    let continuation = if let Some(number) = expected.line_numbers[row] {
                        source_line = Some(number - 1);
                        None
                    } else if row > 0
                        && line.kind == LineKind::Content
                        && line.source == expected.lines[row - 1].source
                    {
                        source_line
                    } else {
                        None
                    };
                    expected_continuations.push(continuation);
                }
                for top in 0..expected.lines.len() {
                    let viewport = session.rendered_viewport(width, top, 4);
                    let end = viewport.first_row + viewport.lines.len();
                    assert_eq!(viewport.total_rows, expected.lines.len());
                    assert_eq!(
                        viewport.lines,
                        expected.lines[top..end],
                        "width {width}, top {top}"
                    );
                    assert_eq!(viewport.line_numbers, expected.line_numbers[top..end]);
                    assert_eq!(
                        viewport.gutter_continuations,
                        expected_continuations[top..end]
                    );
                }
            }
        }
    }

    #[test]
    fn indexed_vertical_motions_match_complete_navigation_without_materializing_rows() {
        let text = "# Heading\n\nA [link](https://example.invalid) with text long enough to wrap at narrow width.\n\n## Second\n\nTail.\n";
        let mut complete = EditorSession::from_text(text);
        let mut indexed = EditorSession::from_text(text);
        complete.render_layout(18);
        indexed.render_layout(18);
        indexed.ensure_rendered_rows(18);
        let ctrl = |character| KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(character),
            },
            mods: Modifiers {
                ctrl: true,
                ..Modifiers::default()
            },
        };
        let motions = [
            key('j'),
            special(KeyCodeKind::Down),
            key('l'),
            special(KeyCodeKind::Right),
            key('h'),
            key('w'),
            key('W'),
            key('e'),
            key('E'),
            key('b'),
            key('B'),
            key('0'),
            key('$'),
            key('{'),
            key('}'),
            special(KeyCodeKind::Home),
            special(KeyCodeKind::End),
            special(KeyCodeKind::Tab),
            special(KeyCodeKind::BackTab),
            key('['),
            key('['),
            key(']'),
            key(']'),
            key('2'),
            key('l'),
            key('k'),
            key('3'),
            key('j'),
            key('G'),
            key('g'),
            key('g'),
            key('2'),
            key('g'),
            key('g'),
            ctrl('d'),
            ctrl('u'),
            ctrl('f'),
            ctrl('b'),
            special(KeyCodeKind::Enter),
            esc(),
            key('~'),
        ];
        for motion in motions {
            assert_eq!(indexed.handle_key(motion), complete.handle_key(motion));
            assert_eq!(indexed.rendered_cursor(), complete.rendered_cursor());
            assert_eq!(indexed.cursor(), complete.cursor());
            assert!(indexed.rendered_state.row_cache.is_some());
            assert!(indexed.rendered_state.layout_cache.is_none());
        }
    }

    #[test]
    fn indexed_line_selection_matches_complete_projection_after_local_edit() {
        let text = "# Café\r\n\r\nA paragraph with a wrapped line and more words.\r\n\r\nTail [link](https://example.invalid).\r\n";
        let mut indexed = EditorSession::from_text(text);
        indexed.render_layout(18);
        indexed
            .jump_to_offset(text.find("wrapped").unwrap())
            .unwrap();
        indexed.ensure_rendered_rows(18);
        indexed.handle_key(key('V'));
        let mut complete = EditorSession::from_text(text);
        complete.render_layout(18);
        complete
            .jump_to_offset(text.find("wrapped").unwrap())
            .unwrap();
        complete.handle_key(key('V'));
        for motion in [key('j'), key('j'), key('k')] {
            assert_eq!(indexed.handle_key(motion), complete.handle_key(motion));
            assert_eq!(indexed.rendered_selection(), complete.rendered_selection());
            assert!(indexed.rendered_state.layout_cache.is_none());
        }
        for edit in [esc(), key('i'), key('x'), esc()] {
            indexed.handle_key(edit);
            complete.handle_key(edit);
        }
        assert_eq!(indexed.document(), complete.document());
        indexed.handle_key(key('V'));
        complete.render_layout(18);
        complete.handle_key(key('V'));
        assert_eq!(indexed.rendered_selection(), complete.rendered_selection());
        let point = RenderedPoint {
            row: complete.rendered_cursor().row + 1,
            column: 3,
        };
        assert_eq!(
            indexed.move_to_rendered_point(point),
            complete.move_to_rendered_point(point)
        );
        assert_eq!(indexed.rendered_selection(), complete.rendered_selection());
        assert!(indexed.rendered_state.layout_cache.is_none());
    }

    #[test]
    fn short_retained_select_shapes_match_complete_without_materializing_layout() {
        let text = "# Café\n\nA long paragraph that wraps across several display rows with **strong** text and a [link](https://example.invalid).\n\n- one `code` item\n- two items\n\n| Key | Value |\n|---|---|\n| α | beta |\n\n```rust\nfn alpha() {\n    let value = 1;\n}\n```\n\nA [^note] reference.\n\n[^note]: footnote body.\n";
        for marker in [
            "long paragraph",
            "two items",
            "| α |",
            "let value",
            "footnote body",
        ] {
            for shape in [key('v'), key('V'), ctrl('v')] {
                let mut indexed = EditorSession::from_text(text);
                let mut complete = EditorSession::from_text(text);
                indexed.render_layout(24);
                complete.render_layout(24);
                let offset = text.find(marker).unwrap();
                indexed.jump_to_offset(offset).unwrap();
                complete.jump_to_offset(offset).unwrap();
                indexed.ensure_rendered_rows(24);
                assert_eq!(indexed.handle_key(shape), complete.handle_key(shape));
                assert_eq!(indexed.rendered_selection(), complete.rendered_selection());
                assert!(indexed.rendered_state.layout_cache.is_none(), "{marker}");
                for motion in [key('j'), key('j'), key('k')] {
                    assert_eq!(indexed.handle_key(motion), complete.handle_key(motion));
                    assert_eq!(indexed.rendered_selection(), complete.rendered_selection());
                    assert!(indexed.rendered_state.layout_cache.is_none(), "{marker}");
                }
                let reads = indexed
                    .rendered_state
                    .row_cache
                    .as_ref()
                    .unwrap()
                    .selection_reads();
                assert!(
                    reads > 0 && reads < 100,
                    "{marker}: {reads} selection rows read"
                );
            }
        }
    }

    #[test]
    fn large_select_motions_and_undo_scale_with_selected_rows() {
        let mixed = crate::realistic_fixtures::generate("mixed", 128 * 1024);
        let mut fence = String::from("# Large fence\n\n```rust\n");
        while fence.len() < 1024 * 1024 {
            fence.push_str("fn example() { let value = 42; }\n");
        }
        fence.push_str("```\n");
        for (text, marker) in [(&mixed, "paragraph"), (&fence, "fn example")] {
            let offset = text.find(marker).unwrap();
            for shape in [key('v'), key('V'), ctrl('v')] {
                let mut indexed = EditorSession::from_text(text);
                let mut complete = EditorSession::from_text(text);
                indexed.render_layout(80);
                complete.render_layout(80);
                indexed.jump_to_offset(offset).unwrap();
                complete.jump_to_offset(offset).unwrap();
                indexed.ensure_rendered_rows(80);
                for input in [shape, key('j'), key('j'), key('k')] {
                    assert_eq!(indexed.handle_key(input), complete.handle_key(input));
                    assert_eq!(indexed.rendered_selection(), complete.rendered_selection());
                    let top = indexed.rendered_cursor().row.saturating_sub(4);
                    let indexed_view = indexed.rendered_viewport(80, top, 12);
                    let complete_view = complete.rendered_viewport(80, top, 12);
                    assert_eq!(indexed_view, complete_view);
                    assert!(indexed.rendered_state.layout_cache.is_none());
                }
                let reads = indexed
                    .rendered_state
                    .row_cache
                    .as_ref()
                    .unwrap()
                    .selection_reads();
                assert!(
                    reads > 0 && reads < 120,
                    "{marker}: {reads} selection row reads"
                );
                assert_eq!(indexed.handle_key(key('d')), complete.handle_key(key('d')));
                assert_eq!(indexed.document(), complete.document());
                assert_eq!(indexed.handle_key(key('u')), complete.handle_key(key('u')));
                assert_eq!(indexed.document(), text.as_str());
                assert_eq!(indexed.rendered_selection(), complete.rendered_selection());
            }
        }
    }

    #[test]
    fn bounded_character_operators_match_complete_projection_across_markdown_constructs() {
        let cases = [
            ("wrapped", "## A\n\nLong words with é and λ that wrap over several display rows.\nSecond source line remains.\n\n## B\nTail.\n", "Long"),
            ("heading", "## A\n\nFirst text.\n\n---\n\n## B\n> Quoted text.\n\n## C\nTail.\n", "First"),
            ("list", "## A\n\n- one\n- two with **bold**\n  - nested\n- three\n\n## B\nTail.\n", "one"),
            ("table", "## A\n\n| Name | Value |\n| --- | --- |\n| é | `λ` |\n| two | three |\n\n## B\nTail.\n", "Name"),
            ("crlf-unicode", "## Café\r\n\r\nα line one\r\nβ line two\r\nγ line three\r\n\r\n## Tail\r\nDone.\r\n", "α line"),
            ("reference", "[id]: /target\n\n## A\n\nA [link][id] and text.\nNext source line.\n\n## B\nTail.\n", "A [link]"),
            ("footnote", "## A\n\nA [^n] and body.\nNext source line.\n\n[^n]: Footnote body.\n\n## B\nTail.\n", "A [^n]"),
            ("fence-boundary", "## A\n\n```rust\nfn main() {}\n```\n\n## B\nTail.\n", "fn main"),
        ];
        for (name, original, marker) in cases {
            for operator in ['d', 'c'] {
                for retained in [false, true] {
                    let mut session = EditorSession::from_text(original);
                    let source = session.document();
                    let width = 18;
                    session.render_layout(width);
                    session
                        .jump_to_offset(source.find(marker).unwrap())
                        .unwrap();
                    if retained {
                        session.ensure_rendered_rows(width);
                    }
                    session.handle_key(key('v'));
                    session.handle_key(key('j'));
                    session.handle_key(key('j'));
                    let selection = session.rendered_selection().unwrap();
                    let layout = session.render_layout(width).clone();
                    let ranges =
                        nav::character_mutation_ranges(&selection.source_ranges, &layout, &source);
                    assert!(!ranges.is_empty(), "{name}");
                    let mut expected = source.clone();
                    for range in ranges.iter().rev() {
                        expected.replace_range(range.clone(), "");
                    }
                    if retained {
                        session.ensure_rendered_rows(width);
                    }
                    session.handle_key(key(operator));
                    assert_eq!(
                        session.document(),
                        expected,
                        "{name} {operator} retained={retained}"
                    );
                    assert_complete_reference(&mut session, width);
                    if operator == 'c' {
                        session.handle_key(esc());
                    }
                    session.handle_key(key('u'));
                    assert_eq!(session.document(), source, "{name} undo");
                    assert_complete_reference(&mut session, width);
                    session.handle_key(ctrl('r'));
                    assert_eq!(session.document(), expected, "{name} redo");
                    assert_complete_reference(&mut session, width);
                }
            }
        }
    }

    #[test]
    fn exact_prose_select_edits_publish_bounded_current_rows() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/kitchen-sink-1mb.md");
        let original = fs::read_to_string(path).unwrap();
        for line in [600, 130] {
            for operator in ['d', 'c'] {
                let mut session = EditorSession::from_text(&original);
                session.render_layout(100);
                let offset = original
                    .split_inclusive('\n')
                    .take(line)
                    .map(str::len)
                    .sum();
                session.jump_to_offset(offset).unwrap();
                session.ensure_rendered_rows(100);
                session.handle_key(key('v'));
                for _ in 0..15 {
                    session.handle_key(key('j'));
                }
                let before_builds = session.rendered_state.layout_builds;
                session.handle_key(key(operator));
                assert_eq!(session.rendered_state.layout_builds, before_builds);
                assert!(session.rendered_state.layout_cache.is_none());
                let rows = session.rendered_state.row_cache.as_ref().unwrap();
                let (rebuilt_rows, _, retained_rows) = rows.work();
                assert!(rebuilt_rows < 100, "{line} {operator}: {rebuilt_rows} rows");
                assert!(
                    retained_rows > 20_000,
                    "{line} {operator}: {retained_rows} retained rows"
                );
                let model_work = session.live.rendered_model_work();
                assert!(!model_work.full_rebuild);
                assert!(model_work.parsed_bytes < 4096);
                assert!(model_work.rebuilt_blocks < 20);
                let top = session.rendered_cursor().row.saturating_sub(8);
                let frame = session.rendered_viewport(100, top, 41);
                assert_eq!(frame.lines.len(), 41);
                assert_eq!(session.rendered_state.layout_builds, before_builds);
                assert_complete_reference(&mut session, 100);
                if operator == 'c' {
                    session.handle_key(esc());
                }
                session.ensure_rendered_rows(100);
                let before_undo_builds = session.rendered_state.layout_builds;
                session.handle_key(key('u'));
                assert_eq!(session.document(), original, "{line} {operator} undo");
                assert!(
                    session.rendered_state.row_cache.is_some(),
                    "{line} {operator} undo discarded retained rows"
                );
                let undo_work = session.live.rendered_model_work();
                assert!(
                    !undo_work.full_rebuild,
                    "{line} {operator} undo rebuilt model"
                );
                assert!(
                    undo_work.parsed_bytes < 4096,
                    "{line} {operator} undo parsed {} bytes",
                    undo_work.parsed_bytes
                );
                let top = session.rendered_cursor().row.saturating_sub(8);
                let frame = session.rendered_viewport(100, top, 41);
                assert_eq!(frame.lines.len(), 41);
                assert_eq!(session.rendered_state.layout_builds, before_undo_builds);
                assert_complete_reference(&mut session, 100);
            }
        }
    }

    #[test]
    fn retained_selection_endpoint_keeps_source_less_line_identity() {
        let text = "# Heading\n\n---\n\nA [paragraph](https://example.invalid) that wraps across two or three rows.\n\n[^a]: Footnote body.\n";
        let mut session = EditorSession::from_text(text);
        let layout = session.render_layout(14).clone();
        session.ensure_rendered_rows(14);
        let mut checked = 0;
        for (row, line) in layout.lines.iter().enumerate() {
            if line.atoms.iter().any(|atom| atom.source.is_some()) {
                continue;
            }
            let point = RenderedPoint { row, column: 0 };
            let expected = nav::line_identity_for_point(point, &layout);
            let endpoint = session.selection_endpoint_at(point);
            assert!(endpoint.atom.is_none());
            assert_eq!(endpoint.line, expected, "row {row}");
            checked += 1;
        }
        assert!(checked > 0);
        assert!(session.rendered_state.layout_cache.is_none());
    }

    #[test]
    fn large_fence_character_delete_keeps_bounded_model_and_rows() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        let mut session = EditorSession::from_text(&original);
        session.render_layout(100);
        let start = original.find("fn summarize_batch_0200").unwrap();
        session.jump_to_offset(start).unwrap();
        session.ensure_rendered_rows(100);
        session.rendered_state.last_work = ProjectionWork::default();
        session.handle_key(key('v'));
        for _ in 0..15 {
            session.handle_key(key('j'));
        }
        let selected_before = session.rendered_selection().unwrap().source_ranges;
        assert!(session.rendered_state.row_cache.is_some());
        session.handle_key(key('d'));
        assert!(session.rendered_state.layout_cache.is_none());
        assert!(
            session.rendered_state.row_cache.is_some(),
            "selected {selected_before:?}, model {:?}, rebuilt rows {}",
            session.live.rendered_model_work(),
            session.rendered_state.last_work.rebuilt_rows
        );
        let reads = session.rendered_state.row_cache.as_ref().unwrap().work().1;
        assert!(reads < 100, "cursor remap read {reads} retained rows");
        assert!(session.rendered_state.row_cache.as_ref().unwrap().work().0 <= 20);
        assert_eq!(session.rendered_state.last_work.rebuilt_rows, 0);
        assert!(!session.live.rendered_model_work().full_rebuild);
        assert_eq!(session.live.rendered_model_work().parsed_bytes, 0);
        let current = session.document();
        let fresh = EditorSession::from_text(&current)
            .render_layout(100)
            .clone();
        let top = session.rendered_cursor().row.saturating_sub(3);
        let actual = session.rendered_viewport(100, top, 12);
        assert_eq!(actual.lines, fresh.lines[top..top + actual.lines.len()]);
        session.handle_key(key('u'));
        assert_eq!(session.document(), original);
    }

    #[test]
    fn large_go_fence_change_matches_complete_source_and_rendered_views() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/kitchen-sink-1mb.md");
        let original = std::fs::read_to_string(path).unwrap();
        let mut session = EditorSession::from_text(&original);
        session.render_layout(100);
        let start = original.find("func summarizeBatch0200").unwrap();
        session.jump_to_offset(start).unwrap();
        session.ensure_rendered_rows(100);
        session.handle_key(key('v'));
        for _ in 0..15 {
            session.handle_key(key('j'));
        }
        session.handle_key(key('c'));
        assert_eq!(session.mode(), Mode::Insert);
        assert!(session.rendered_state.row_cache.is_some());
        assert!(!session.live.rendered_model_work().full_rebuild);
        assert_eq!(session.live.rendered_model_work().parsed_bytes, 0);
        let changed = session.document();
        assert_ne!(changed, original);
        let mut fresh = EditorSession::from_text(&changed);
        let expected_layout = fresh.render_layout(100).clone();
        assert_eq!(session.live.rendered_model(), fresh.live.rendered_model());
        let actual_layout = session.render_layout(100);
        assert_eq!(actual_layout.lines.len(), expected_layout.lines.len());
        for (row, (actual, expected)) in actual_layout
            .lines
            .iter()
            .zip(&expected_layout.lines)
            .enumerate()
        {
            assert!(
                actual == expected,
                "row {row}: text {:?} vs {:?}, spans {:?} vs {:?}, source {:?} vs {:?}",
                actual.styled.text,
                expected.styled.text,
                actual.styled.spans,
                expected.styled.spans,
                actual.source,
                expected.source,
            );
        }
        assert_eq!(actual_layout.line_numbers, expected_layout.line_numbers);
        assert_eq!(actual_layout.jump_targets, expected_layout.jump_targets);
        assert_eq!(actual_layout.link_index, expected_layout.link_index);
        let source_top = session.cursor().0.saturating_sub(3);
        let viewport = Viewport {
            top_line: source_top,
            height: 12,
            width: 100,
            wrap: true,
            left_col: 0,
            skip_rows: 0,
        };
        assert_eq!(
            session.render_source(viewport).lines,
            fresh.render_source(viewport).lines,
        );
        session.handle_key(esc());
        session.handle_key(key('u'));
        assert_eq!(session.document(), original);
    }

    #[test]
    fn inserted_fence_delimiter_rebuilds_before_publishing_rendered_rows() {
        let original = "# Heading\n\n```rust\nfn alpha() {}\nfn beta() {}\n```\n\nTail.\n";
        let mut session = EditorSession::from_text(original);
        session.render_layout(80);
        session.ensure_rendered_rows(80);
        let at = original.find("fn beta").unwrap();
        let mutation = session.live.replace_range(at..at, "```\n").unwrap();
        session.translate_vim_effects(mutation.effects);
        assert!(session.rendered_state.row_cache.is_none());
        assert!(session.live.rendered_model_work().full_rebuild);
        let changed = session.document();
        let mut fresh = EditorSession::from_text(&changed);
        assert_eq!(session.render_layout(80), fresh.render_layout(80));
        assert_eq!(
            session
                .live
                .highlighter()
                .highlight_lines(0..session.line_count()),
            fresh
                .live
                .highlighter()
                .highlight_lines(0..fresh.line_count()),
        );
    }

    #[test]
    fn indexed_source_jumps_match_complete_mapping_without_materializing_rows() {
        let text = "# Café\r\n\r\nA wrapped paragraph with α and [link](https://example.invalid).\r\n\r\nTail.\r\n";
        let mut indexed = EditorSession::from_text(text);
        let mut complete = EditorSession::from_text(text);
        indexed.render_layout(16);
        complete.render_layout(16);
        indexed.ensure_rendered_rows(16);
        let edit_at = indexed.document().find("wrapped").unwrap();
        indexed.jump_to_offset(edit_at).unwrap();
        complete.jump_to_offset(edit_at).unwrap();
        for edit in [key('i'), key('x'), esc()] {
            indexed.handle_key(edit);
            complete.handle_key(edit);
        }
        complete.render_layout(16);
        assert!(indexed.rendered_state.row_cache.is_some());
        let current = indexed.document();
        assert_eq!(current, complete.document());
        for offset in [
            current.find("Café").unwrap(),
            current.find("wrapped").unwrap(),
            current.find('α').unwrap(),
            current.find("Tail").unwrap(),
            current.find("\n\n").unwrap() + 1,
            current.len(),
        ] {
            assert_eq!(
                indexed.jump_to_offset(offset),
                complete.jump_to_offset(offset)
            );
            assert_eq!(indexed.rendered_cursor(), complete.rendered_cursor());
            assert_eq!(indexed.cursor(), complete.cursor());
            assert!(
                indexed.rendered_state.row_cache.is_some(),
                "offset {offset} of {}",
                current.len()
            );
            assert!(indexed.rendered_state.layout_cache.is_none());
        }
    }

    #[test]
    fn indexed_search_matches_complete_prompt_and_repeat_after_local_edit() {
        let text = "# Heading\n\nalpha text\n\nbeta alpha\n\nTail.\n";
        let mut indexed = EditorSession::from_text(text);
        let mut complete = EditorSession::from_text(text);
        indexed.render_layout(20);
        complete.render_layout(20);
        indexed.jump_to_offset(text.find("alpha").unwrap()).unwrap();
        complete
            .jump_to_offset(text.find("alpha").unwrap())
            .unwrap();
        indexed.ensure_rendered_rows(20);
        for edit in [key('i'), key('x'), esc()] {
            indexed.handle_key(edit);
            complete.handle_key(edit);
        }
        complete.render_layout(20);
        for input in [
            key('/'),
            key('b'),
            key('e'),
            key('t'),
            key('a'),
            special(KeyCodeKind::Enter),
            key('n'),
            key('N'),
        ] {
            assert_eq!(indexed.handle_key(input), complete.handle_key(input));
            assert_eq!(indexed.rendered_cursor(), complete.rendered_cursor());
            assert_eq!(indexed.rendered_search(), complete.rendered_search());
            assert!(indexed.rendered_state.row_cache.is_some());
            assert!(indexed.rendered_state.layout_cache.is_none());
        }
    }

    #[test]
    fn work_counters_expose_current_full_rebuild_and_source_propagation() {
        let text = format!(
            "---\ntitle: Work\n---\n\n{}",
            "## Heading\n\nA paragraph with [link](https://example.invalid).\n\n```rust\nlet n = 42;\n```\n\n"
                .repeat(512)
        );
        let mut session = EditorSession::from_text(&text);
        let before = session.render_layout(76).lines.len();
        assert!(before > 2_000);
        let prior_parses = session.live.highlighter().work_snapshot().0;
        let offset = text.find("A paragraph").unwrap();
        session.jump_to_offset(offset).unwrap();
        session.handle_key(key('i'));
        session.handle_key(key('#'));
        let (
            parse_count,
            changed_ranges,
            changed_bytes,
            injections,
            injection_scans,
            reference_scans,
        ) = session.live.highlighter().work_snapshot();
        assert!(
            parse_count > prior_parses,
            "structural edit must invoke the parser"
        );
        assert!(changed_ranges <= session.line_count());
        assert!(changed_bytes <= session.document().len());
        assert!(injections > 0);
        assert!(injection_scans > 1);
        assert!(reference_scans > 1);
        let rebuilt_rows = session.render_layout(76).lines.len();
        let work = &session.rendered_state.last_work;
        assert_eq!(work.rebuilt_rows, rebuilt_rows);
        assert_eq!(work.reused_rows, 0);
        assert!(work.rebuilt_blocks > 512);
        assert_eq!(work.reused_blocks, 0);
    }

    #[test]
    #[ignore = "exact 1 MiB diagnostic is run by the acceptance benchmark target"]
    fn acceptance_1mb_mutation_work_profile() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/kitchen-sink-1mb.md");
        let text = fs::read_to_string(path).unwrap();
        assert_eq!(text.len(), 1_048_722);
        for (language, source_line) in [("rust", 3000), ("go", 20000)] {
            let mut session = EditorSession::from_text(&text);
            session.render_layout(100);
            let offset = text
                .split_inclusive('\n')
                .take(source_line)
                .map(str::len)
                .sum();
            session.jump_to_offset(offset).unwrap();
            session.handle_key(key('v'));
            for _ in 0..15 {
                session.handle_key(key('j'));
            }
            let before = session.live.highlighter().work_snapshot();
            let started = std::time::Instant::now();
            session.handle_key(key('d'));
            let input_ns = started.elapsed().as_nanos();
            assert_eq!(session.mode(), Mode::Normal);
            let after = session.live.highlighter().work_snapshot();
            let model = session.live.rendered_model_work();
            let started = std::time::Instant::now();
            session.render_layout(100);
            let render_ns = started.elapsed().as_nanos();
            println!(
                "MUTATION\t{language}\t{input_ns}\t{render_ns}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                after.0 - before.0,
                after.1,
                after.2,
                after.3,
                after.4,
                after.5,
                model.rebuilt_blocks,
                model.parsed_bytes,
                model.full_rebuild
            );
            session.handle_key(key('u'));
            assert_eq!(session.live.text(), text);
        }
    }

    #[test]
    #[ignore = "exact 1 MiB multi-range feasibility diagnostic"]
    fn acceptance_1mb_prose_batch_profile() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/kitchen-sink-1mb.md");
        let original = fs::read_to_string(path).unwrap();
        assert_eq!(original.len(), 1_048_722);
        for first_line in [600, 130] {
            let mut session = EditorSession::from_text(&original);
            session.render_layout(100);
            session.ensure_rendered_rows(100);
            let offset = original
                .split_inclusive('\n')
                .take(first_line)
                .map(str::len)
                .sum();
            session.jump_to_offset(offset).unwrap();
            session.handle_key(key('v'));
            for _ in 0..15 {
                session.handle_key(key('j'));
            }
            let selection = session.rendered_selection().unwrap();
            let ranges = nav::character_mutation_ranges(
                &selection.source_ranges,
                session.render_layout(100),
                &original,
            );
            let edits = ranges
                .iter()
                .rev()
                .map(|range| crate::vim::TextEdit {
                    range: range.clone(),
                    new_text_len: 0,
                    new_text: String::new(),
                })
                .collect::<Vec<_>>();
            let mut expected = original.clone();
            for edit in &edits {
                expected.replace_range(edit.range.clone(), "");
            }
            let mut source = crate::syntax::Highlighter::new(&original);
            let start = std::time::Instant::now();
            source.apply_edit(&edits);
            let source_ns = start.elapsed().as_nanos();
            assert_eq!(source.text(), expected);
            let fresh = crate::syntax::Highlighter::new(&expected);
            let changed_lines = first_line.saturating_sub(2)..first_line + 18;
            assert_eq!(
                source.highlight_lines(changed_lines.clone()),
                fresh.highlight_lines(changed_lines)
            );
            let old_window_start = original[..ranges[0].start]
                .rfind("\n## ")
                .map_or(0, |at| at + 1);
            let old_window_end = original[ranges.last().unwrap().end..]
                .find("\n## ")
                .map_or(original.len(), |at| ranges.last().unwrap().end + at + 1);
            let removed_bytes = edits
                .iter()
                .map(|edit| edit.range.end - edit.range.start)
                .sum::<usize>();
            let new_window = old_window_start..old_window_end - removed_bytes;
            let fragment_started = std::time::Instant::now();
            let fragment = crate::syntax::Highlighter::new(&expected[new_window.clone()]);
            let fragment_ns = fragment_started.elapsed().as_nanos();
            let first_window_line = expected[..new_window.start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            let window_lines = expected[new_window.clone()]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count();
            assert_eq!(
                fragment.highlight_lines(0..window_lines),
                fresh.highlight_lines(first_window_line..first_window_line + window_lines),
            );
            let start = std::time::Instant::now();
            let full_model =
                BlockModel::build(&expected, crate::frontmatter::front_matter_span(&expected));
            let full_model_ns = start.elapsed().as_nanos();
            session.ensure_rendered_rows(100);
            let start = std::time::Instant::now();
            session.handle_key(key('d'));
            let current_handler_ns = start.elapsed().as_nanos();
            let [vim_ns, prepare_ns, source_refresh_ns, model_refresh_ns] =
                session.live.selection_profile_ns();
            let publish_profile = session.rendered_state.last_publish_profile_ns;
            let retained_published = session.rendered_state.last_publish_used_retained;
            let select_profile = session.rendered_state.last_select_operator_profile_ns;
            assert_eq!(session.document(), expected);
            assert_eq!(session.live.rendered_model(), &full_model);
            let model_work = session.live.rendered_model_work();
            let start = std::time::Instant::now();
            assert_complete_reference(&mut session, 100);
            let full_frame_oracle_ns = start.elapsed().as_nanos();
            session.ensure_rendered_rows(100);
            let start = std::time::Instant::now();
            session.handle_key(key('u'));
            let undo_handler_ns = start.elapsed().as_nanos();
            let start = std::time::Instant::now();
            let top = session.rendered_cursor().row.saturating_sub(8);
            let _ = session.rendered_viewport(100, top, 41);
            let undo_frame_ns = start.elapsed().as_nanos();
            println!(
                "PROSE-BATCH\t{first_line}\t{}\t{source_ns}\t{full_model_ns}\t{fragment_ns}\t{}\t{current_handler_ns}\t{vim_ns}\t{prepare_ns}\t{source_refresh_ns}\t{model_refresh_ns}\t{full_frame_oracle_ns}\t{undo_handler_ns}\t{undo_frame_ns}\t{}\t{}\t{}\t{publish_profile:?}\t{retained_published}\t{select_profile:?}\t{ranges:?}",
                edits.len(),
                new_window.end - new_window.start,
                model_work.rebuilt_blocks,
                model_work.parsed_bytes,
                model_work.full_rebuild,
            );
        }
    }

    #[test]
    #[ignore = "exact 1 MiB feasibility diagnostic is run by its benchmark target"]
    fn acceptance_1mb_contiguous_fence_edit_profile() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/kitchen-sink-1mb.md");
        let original = fs::read_to_string(path).unwrap();
        assert_eq!(original.len(), 1_048_722);
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
            let mut vim = crate::vim::VimCore::new(&original);
            let mut syntax = crate::syntax::Highlighter::new(&original);
            let prehighlight_started = std::time::Instant::now();
            let _ = syntax.highlight_lines(first_line..first_line + 15);
            let prehighlight_ns = prehighlight_started.elapsed().as_nanos();
            let started = std::time::Instant::now();
            let edits = vim.replace_range(start..end, "").unwrap();
            let vim_ns = started.elapsed().as_nanos();
            assert_eq!(edits.len(), 1);
            let expected = format!("{}{}", &original[..start], &original[end..]);
            assert_eq!(vim.text(), expected);
            let before = syntax.work_snapshot();
            let started = std::time::Instant::now();
            syntax.apply_edit(&edits);
            let source_ns = started.elapsed().as_nanos();
            let after = syntax.work_snapshot();
            let started = std::time::Instant::now();
            let actual = syntax.highlight_lines(first_line..first_line + 15);
            let highlight_ns = started.elapsed().as_nanos();
            let fresh = crate::syntax::Highlighter::new(&expected);
            assert_eq!(actual, fresh.highlight_lines(first_line..first_line + 15));
            vim.handle_key(key('u'));
            assert_eq!(vim.text(), original);
            println!(
                "CONTIGUOUS\t{language}\t{vim_ns}\t{source_ns}\t{highlight_ns}\t{prehighlight_ns}\t{}\t{}\t{}",
                after.0 - before.0,
                after.2,
                after.4 - before.4,
            );
        }
    }

    #[test]
    fn save_faults_preserve_live_text_and_undo_and_distinguish_commit() {
        for (after_commit, expected_bytes) in [(false, "original\n"), (true, "Xoriginal\n")] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("file.md");
            fs::write(&path, "original\n").unwrap();
            let mut session = EditorSession::open(&path).unwrap();
            session.handle_key(key('i'));
            session.handle_key(key('X'));
            session.handle_key(esc());
            let cursor = session.cursor();
            session.render_layout(40);
            let layout_builds = session.rendered_state.layout_builds;
            let mut operations = FaultSave {
                delegate: FileSystemAtomicSave,
                fail_parent_sync: after_commit,
            };
            let error = session
                .save_with_operations(None, false, &mut operations)
                .unwrap_err();
            assert_eq!(
                matches!(error, SaveError::CommittedUncertain(_)),
                after_commit
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), expected_bytes);
            assert_eq!(session.document(), "Xoriginal\n");
            assert_eq!(session.cursor(), cursor);
            assert!(session.is_dirty());
            session.render_layout(40);
            assert_eq!(session.rendered_state.layout_builds, layout_builds);
            session.handle_key(key('u'));
            assert_eq!(session.document(), "original\n");
            if after_commit {
                assert!(session.is_dirty(), "durability uncertainty survives undo");
            }
        }
    }

    #[test]
    fn saves_preserve_layout_while_text_and_geometry_changes_rebuild() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(&path, "# heading\n\noriginal\n").unwrap();
        let mut session = EditorSession::open(&path).unwrap();
        let original = session.render_layout(40).clone();
        assert_eq!(session.rendered_state.layout_builds, 1);

        session.save(None, false).unwrap();
        assert!(!session.is_dirty());
        assert_eq!(session.render_layout(40), &original);
        assert_eq!(session.rendered_state.layout_builds, 1);

        session.handle_key(key('i'));
        session.handle_key(key('X'));
        session.handle_key(esc());
        assert!(session.is_dirty());
        let edited = session.render_layout(40).clone();
        assert_ne!(edited, original);
        assert_eq!(session.rendered_state.layout_builds, 2);
        session.save(None, false).unwrap();
        assert!(!session.is_dirty());
        assert_eq!(session.render_layout(40), &edited);
        assert_eq!(session.rendered_state.layout_builds, 2);

        session.handle_key(key('i'));
        session.handle_key(key('Y'));
        session.handle_key(esc());
        let version_checked_layout = session.render_layout(40).clone();
        assert!(session.is_dirty());
        assert_eq!(session.rendered_state.layout_builds, 3);
        let version = crate::DiskVersion::observe(&path).unwrap();
        session.save_if_version(None, &version).unwrap();
        assert!(!session.is_dirty());
        assert_eq!(session.render_layout(40), &version_checked_layout);
        assert_eq!(session.rendered_state.layout_builds, 3);

        let retarget = dir.path().join("retarget.md");
        session.save(Some(&retarget), false).unwrap();
        assert_eq!(session.path(), Some(retarget.as_path()));
        assert_eq!(session.render_layout(40), &version_checked_layout);
        assert_eq!(session.rendered_state.layout_builds, 3);

        let copy = dir.path().join("copy.md");
        session.save_copy(&copy).unwrap();
        assert_eq!(session.render_layout(40), &version_checked_layout);
        assert_eq!(session.rendered_state.layout_builds, 3);

        session.render_layout(24);
        assert_eq!(session.rendered_state.layout_builds, 4);
        let external = "# changed externally\n\nreplacement\n";
        fs::write(&retarget, external).unwrap();
        let version = match session.disk_state() {
            crate::DiskState::Modified { version } => version,
            state => panic!("expected modified disk state, got {state:?}"),
        };
        assert!(session.save(None, false).is_err());
        assert_eq!(session.rendered_state.layout_builds, 4);
        session.reload_from_disk(&version).unwrap();
        assert_eq!(session.document(), external);
        assert_ne!(session.render_layout(24), &edited);
        assert_eq!(session.rendered_state.layout_builds, 1);
    }

    #[test]
    fn stale_version_save_preserves_dirty_state_and_rendered_layout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(&path, "original\n").unwrap();
        let mut session = EditorSession::open(&path).unwrap();
        session.handle_key(key('i'));
        session.handle_key(key('X'));
        session.handle_key(esc());
        let layout = session.render_layout(40).clone();
        let stale = crate::DiskVersion::observe(&path).unwrap();
        fs::write(&path, "other contents\n").unwrap();
        assert!(session.save_if_version(None, &stale).is_err());
        assert!(session.is_dirty());
        assert_eq!(session.render_layout(40), &layout);
        assert_eq!(session.rendered_state.layout_builds, 1);
    }

    #[test]
    fn session_starts_in_rendered_normal() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), (0, 0));
        assert!(!session.render_layout(31).lines.is_empty());
        assert_eq!(session.rendered_cursor_line(), 0);
    }

    #[test]
    fn source_frame_tracks_cursor_on_trailing_empty_lines() {
        let mut session = EditorSession::from_text("");
        session.handle_key(key('i'));
        for character in "# foo".chars() {
            session.handle_key(key(character));
        }

        for expected_line in 1..=2 {
            session.handle_key(special(KeyCodeKind::Enter));
            let frame = session.render_source(Viewport {
                top_line: 0,
                height: 5,
                width: 80,
                wrap: true,
                left_col: 0,
                skip_rows: 0,
            });

            assert_eq!(session.cursor(), (expected_line, 0));
            assert_eq!(frame.cursor, (expected_line as u16, 0));
            assert_eq!(frame.line_numbers[expected_line], Some(expected_line + 1));
            assert_eq!(frame.lines[expected_line].text, "");
        }
    }

    #[test]
    fn rendered_navigation_updates_canonical_source_cursor() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        session.render_layout(40);
        session.handle_key(key('j'));
        assert_eq!(session.cursor(), (1, 0));
        session.handle_key(key('j'));
        assert_eq!(session.cursor().0, 2);
    }

    #[test]
    fn navigation_and_frames_reuse_materialization_layout_and_line_index_work() {
        let text = "plain paragraph content for cached work counters\n\n".repeat(500);
        let mut session = EditorSession::from_text(&text);
        session.render_layout(80);
        assert_eq!(session.rendered_state.layout_builds, 1);
        session.live.reset_work_counters();

        for input in [
            key('j'),
            key('k'),
            key('2'),
            key('0'),
            key('j'),
            key('G'),
            key('g'),
            key('g'),
        ] {
            session.handle_key(input);
            session.render_layout(80);
        }
        assert_eq!(session.live.work_counters(), (0, 0));
        assert_eq!(session.rendered_state.layout_builds, 1);

        session.handle_key(key('i'));
        assert_eq!(session.mode(), Mode::Insert);
        session.live.reset_work_counters();
        let line_index_builds = crate::syntax::line_index_build_count();
        for _ in 0..20 {
            session.handle_key(special(KeyCodeKind::Down));
        }
        let top_line = session.cursor().0.saturating_sub(5);
        for _ in 0..3 {
            let _ = session.render_source(Viewport {
                top_line,
                height: 10,
                width: 80,
                wrap: true,
                left_col: 0,
                skip_rows: 0,
            });
        }
        assert_eq!(session.live.work_counters(), (0, 0));
        assert_eq!(crate::syntax::line_index_build_count(), line_index_builds);

        session.handle_key(key('x'));
        let _ = session.render_source(Viewport {
            top_line,
            height: 10,
            width: 80,
            wrap: true,
            left_col: 0,
            skip_rows: 0,
        });
        assert_eq!(session.live.work_counters(), (1, 1));
        assert_eq!(crate::syntax::line_index_build_count(), line_index_builds);

        session.handle_key(esc());
        session.live.reset_work_counters();
        session.render_layout(80);
        assert_eq!(session.rendered_state.layout_builds, 2);
        assert_eq!(session.live.work_counters(), (1, 0));
    }

    #[test]
    fn first_actual_width_preserves_source_anchor() {
        let text = "# Heading\n\nA paragraph with enough words to wrap at a narrow width.\n";
        let mut session = EditorSession::from_text(text);
        session.live.jump_to(2, 12);
        let before = session.cursor();
        session.render_layout(23);
        assert_eq!(session.cursor(), before);
        session.render_layout(61);
        assert_eq!(session.cursor(), before);
    }

    #[test]
    fn insert_select_command_roundtrip_preserves_source_position() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        session.render_layout(40);
        session.handle_key(key('j'));
        session.handle_key(key('j'));
        let body = session.cursor();

        session.handle_key(key('V'));
        assert_eq!(session.mode(), Mode::Select);
        session.handle_key(esc());
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), body);

        session.handle_key(key(':'));
        assert_eq!(session.mode(), Mode::Command);
        session.handle_key(esc());
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), body);

        session.handle_key(key('i'));
        assert_eq!(session.mode(), Mode::Insert);
        session.handle_key(esc());
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), body);
    }

    #[test]
    fn select_yank_is_non_destructive_and_line_aligned() {
        let text = "# Heading\n\nFirst\nSecond\n";
        let mut session = EditorSession::from_text(text);
        session.render_layout(40);
        session.handle_key(key('V'));
        session.handle_key(key('j'));
        let selection = session.rendered_selection().unwrap();
        assert_eq!(selection.source_ranges, vec![0..11]);
        assert_eq!(&text[selection.source_ranges[0].clone()], "# Heading\n\n");
        session.handle_key(key('y'));
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.document(), text);
    }

    #[test]
    fn interaction_state_select_carries_complete_endpoints() {
        let mut session = EditorSession::from_text("alpha beta\n");
        session.render_layout(40);
        session.handle_key(key('v'));
        let SessionMode::Select(selection) = &session.session_mode else {
            panic!("Select mode must carry its state");
        };
        assert_eq!(selection.anchor, selection.active);
        assert_eq!(selection.anchor.point, session.rendered_cursor());
        assert_eq!(selection.anchor.source, session.cursor());
        assert!(selection.anchor.atom.is_some());
        assert!(selection.anchor.line.is_some());
    }

    #[test]
    fn selection_state_swap_exchanges_complete_endpoint_identity() {
        let mut session = EditorSession::from_text("alpha beta gamma\n");
        session.render_layout(40);
        session.handle_key(key('v'));
        session.handle_key(key('w'));
        let SessionMode::Select(before) = &session.session_mode else {
            panic!("selection missing");
        };
        let before = before.clone();
        session.handle_key(key('o'));
        let SessionMode::Select(after) = &session.session_mode else {
            panic!("selection missing after swap");
        };
        assert_eq!(after.anchor, before.active);
        assert_eq!(after.active, before.anchor);
        assert_eq!(session.cursor(), after.active.source);
        assert_eq!(session.rendered_cursor(), after.active.point);
    }

    #[test]
    fn selection_state_shape_switch_refreshes_only_shape_specific_projection() {
        let mut session = EditorSession::from_text("alpha beta\nsecond\n");
        session.render_layout(40);
        session.handle_key(key('v'));
        session.handle_key(key('w'));
        let SessionMode::Select(before) = &session.session_mode else {
            panic!("selection missing");
        };
        let endpoints = (before.anchor.clone(), before.active.clone());
        session.handle_key(key('V'));
        let SessionMode::Select(after) = &session.session_mode else {
            panic!("selection missing");
        };
        assert_eq!((&after.anchor, &after.active), (&endpoints.0, &endpoints.1));
        assert!(matches!(after.kind, SelectionKind::Line));
    }

    #[test]
    fn selection_kind_switch_move_remap_and_return_never_reuses_character_cache() {
        let mut session = EditorSession::from_text("alpha beta gamma delta\nsecond line\n");
        session.render_layout(40);
        session.handle_key(key('v'));
        session.handle_key(key('w'));
        if let SessionMode::Select(ActiveSelection {
            kind: SelectionKind::Character { ranges },
            ..
        }) = &mut session.session_mode
        {
            *ranges = std::iter::once(usize::MAX - 1..usize::MAX).collect();
        }
        session.handle_key(key('V'));
        session.handle_key(key('j'));
        session.render_layout(12);
        session.handle_key(key('v'));
        let second = session.rendered_selection().unwrap().source_ranges;
        assert!(!second.iter().any(|range| range.end == usize::MAX));
        let SessionMode::Select(selection) = &session.session_mode else {
            panic!("selection missing");
        };
        assert!(matches!(selection.kind, SelectionKind::Character { .. }));
    }

    #[test]
    fn interaction_state_teardown_is_atomic_for_every_exit() {
        for exit in [esc(), ctrl('c'), key('v'), key('y')] {
            let mut session = EditorSession::from_text("alpha beta\n");
            session.render_layout(40);
            session.handle_key(key('v'));
            session.handle_key(exit);
            assert!(matches!(session.session_mode, SessionMode::CoreDriven));
            assert_eq!(session.mode(), Mode::Normal);
            assert!(session.rendered_selection().is_none());
            assert_eq!(
                session.rendered_state.register_input,
                RegisterInput::Default
            );
        }
    }

    #[test]
    fn core_driven_public_mode_tracks_vim_transitions() {
        let mut session = EditorSession::from_text("text");
        session.render_layout(20);
        session.handle_key(key('i'));
        assert!(matches!(session.session_mode, SessionMode::CoreDriven));
        assert_eq!(session.live.mode(), crate::vim::Mode::Insert);
        assert_eq!(session.mode(), Mode::Insert);
        session.handle_key(esc());
        assert_eq!(session.live.mode(), crate::vim::Mode::Normal);
        assert_eq!(session.mode(), Mode::Normal);
    }

    #[test]
    fn register_input_is_consumed_or_cleared_atomically() {
        let mut session = EditorSession::from_text("alpha beta\n");
        session.render_layout(40);
        session.handle_key(key('v'));
        session.handle_key(key('"'));
        assert_eq!(
            session.rendered_state.register_input,
            RegisterInput::AwaitingName
        );
        session.handle_key(key('a'));
        assert_eq!(
            session.rendered_state.register_input,
            RegisterInput::Selected(Register::Named('a'))
        );
        session.handle_key(key('y'));
        assert_eq!(
            session.rendered_state.register_input,
            RegisterInput::Default
        );

        session.handle_key(key('"'));
        session.handle_key(key('!'));
        assert_eq!(
            session.rendered_state.register_input,
            RegisterInput::Default
        );

        session.handle_key(key('"'));
        session.handle_key(ctrl('r'));
        assert_eq!(
            session.rendered_state.register_input,
            RegisterInput::Default
        );
    }

    #[test]
    fn deleting_after_normalized_inline_code_boundary_removes_only_selected_raw_character() {
        let mut session = EditorSession::from_text("`alpha\nbeta`\n");
        session.render_layout(40);
        let beta = session.document().find("beta").unwrap();
        let point = nav::point_for_source_range(
            &(beta..beta + 1),
            session.rendered_state.layout_cache.as_ref().unwrap(),
        )
        .expect("beta must have an exact rendered source atom");
        session.rendered_state.cursor = RenderedCursor::at(point);
        session.live.jump_to(1, 0);

        session.handle_key(key('v'));
        session.handle_key(key('d'));

        assert_eq!(session.document(), "`alpha\neta`\n");
        assert_eq!(session.mode(), Mode::Normal);

        let mut literal = EditorSession::from_text("`left\\|right`\n");
        literal.render_layout(40);
        let after_pipe = literal.document().find("right").unwrap();
        let point = nav::point_for_source_range(
            &(after_pipe..after_pipe + 1),
            literal.rendered_state.layout_cache.as_ref().unwrap(),
        )
        .expect("character after literal pipe must have an exact source atom");
        literal.rendered_state.cursor = RenderedCursor::at(point);
        literal.live.jump_to(0, after_pipe);
        literal.handle_key(key('v'));
        literal.handle_key(key('d'));
        assert_eq!(literal.document(), "`left\\|ight`\n");
    }

    #[test]
    fn deleting_after_prose_non_escape_removes_only_selected_raw_character() {
        let mut session = EditorSession::from_text("left\\qright\n");
        session.render_layout(40);
        let after_non_escape = session.document().find("right").unwrap();
        let point = nav::point_for_source_range(
            &(after_non_escape..after_non_escape + 1),
            session.rendered_state.layout_cache.as_ref().unwrap(),
        )
        .expect("character after non-escape must have an exact source atom");
        session.rendered_state.cursor = RenderedCursor::at(point);
        session.live.jump_to(0, after_non_escape);
        session.handle_key(key('v'));
        session.handle_key(key('d'));
        assert_eq!(session.document(), "left\\qight\n");
    }

    #[test]
    fn selection_transition_sequences_preserve_public_invariants() {
        let mut session = EditorSession::from_text("alpha **beta** gamma\nsecond row\n");
        for width in [40, 12, 28] {
            session.render_layout(width);
            session.handle_key(key('v'));
            for motion in [key('w'), key('o'), key('V'), key('j'), key('v')] {
                session.handle_key(motion);
                if session.mode() == Mode::Select {
                    let selection = session.rendered_selection().expect("coherent selection");
                    for range in &selection.source_ranges {
                        assert!(session.document().is_char_boundary(range.start));
                        assert!(session.document().is_char_boundary(range.end));
                    }
                }
            }
            session.handle_key(esc());
            assert!(session.rendered_selection().is_none());
        }
    }

    #[test]
    fn rendered_search_prompt_carries_draft_and_fixed_origin() {
        let mut session = EditorSession::from_text("zero\n\nalpha two\n");
        session.render_layout(40);
        let origin = session.rendered_state.cursor;
        session.handle_key(key('/'));
        session.handle_key(key('a'));
        let RenderedSearchState::Prompt {
            draft,
            origin: stored_origin,
            ..
        } = &session.rendered_state.search
        else {
            panic!("search prompt missing");
        };
        assert_eq!(draft.pattern, "a");
        assert_eq!(*stored_origin, origin);
        assert_ne!(session.rendered_state.cursor, origin);
    }

    #[test]
    fn rendered_search_submit_cancel_repeat_and_mode_change_are_atomic() {
        let mut session = EditorSession::from_text("alpha x alpha\n");
        session.render_layout(40);
        session.handle_key(key('/'));
        for c in "alpha".chars() {
            session.handle_key(key(c));
        }
        session.handle_key(special(KeyCodeKind::Enter));
        assert!(matches!(
            session.rendered_state.search,
            RenderedSearchState::Inactive { .. }
        ));
        session.handle_key(key('/'));
        session.handle_key(key('x'));
        session.handle_key(esc());
        assert_eq!(session.rendered_search().unwrap().pattern, "alpha");
        session.handle_key(key('/'));
        session.enter_insert_from_rendered(RenderedExitAction::Insert);
        assert!(matches!(
            session.rendered_state.search,
            RenderedSearchState::Inactive { .. }
        ));
    }

    #[test]
    fn rendered_search_cancel_preserves_previous_repeat_target() {
        let mut session = EditorSession::from_text("alpha\n\nbeta\n\nalpha\n\nbeta\n");
        session.render_layout(40);
        session.handle_key(key('/'));
        for c in "alpha".chars() {
            session.handle_key(key(c));
        }
        session.handle_key(special(KeyCodeKind::Enter));
        session.handle_key(key('/'));
        for c in "beta".chars() {
            session.handle_key(key(c));
        }
        session.handle_key(esc());
        assert_eq!(session.rendered_search().unwrap().pattern, "alpha");
        let before = session.rendered_cursor();
        session.handle_key(key('n'));
        assert_ne!(session.rendered_cursor(), before);
        session.handle_key(key('N'));
        assert_eq!(session.rendered_search().unwrap().pattern, "alpha");
    }

    #[test]
    fn from_text_normalizes_live_text_once() {
        let session = EditorSession::from_text("one\r\ntwo\rthree\r\n");
        assert_eq!(session.document(), "one\ntwo\nthree\n");
        assert_eq!(session.live.highlighter().text(), session.document());
        assert_eq!(session.line_ending(), LineEnding::CrLf);
        assert!(session.has_final_newline());
    }

    #[test]
    fn session_front_matter_tracks_unsaved_insert_edits() {
        let mut session = EditorSession::from_text("");
        session.render_layout(40);
        session.handle_key(key('i'));
        session.insert_paste("---\ntitle: live\n---\n");
        assert!(session.front_matter().is_ok());
        assert_eq!(
            session
                .front_matter()
                .value()
                .and_then(|value| value.get("title")),
            Some(&crate::frontmatter::Value::str("live".to_string()))
        );
        assert_eq!(session.live.highlighter().text(), session.document());
    }

    #[test]
    fn session_front_matter_tracks_substitute_undo_and_redo() {
        let mut session = EditorSession::from_text("---\ntitle: old\n---\n\nbody\n");
        session.render_layout(40);
        for input in [
            key(':'),
            key('%'),
            key('s'),
            key('/'),
            key('o'),
            key('l'),
            key('d'),
            key('/'),
            key('n'),
            key('e'),
            key('w'),
            key('/'),
            special(KeyCodeKind::Enter),
        ] {
            session.handle_key(input);
        }
        assert_eq!(
            session
                .front_matter()
                .value()
                .and_then(|value| value.get("title")),
            Some(&crate::frontmatter::Value::str("new".to_string()))
        );
        session.handle_key(key('u'));
        assert_eq!(
            session
                .front_matter()
                .value()
                .and_then(|value| value.get("title")),
            Some(&crate::frontmatter::Value::str("old".to_string()))
        );
        session.handle_key(ctrl('r'));
        assert_eq!(
            session
                .front_matter()
                .value()
                .and_then(|value| value.get("title")),
            Some(&crate::frontmatter::Value::str("new".to_string()))
        );
    }

    #[test]
    fn all_mutation_entry_points_refresh_derived_state() {
        let mut session = EditorSession::from_text("---\ntitle: alpha\n---\n\nalpha wrng\n");
        let mut builder =
            oom_spell::SpellEngineBuilder::new(vec!["alpha\ntitle\nprefix\n".to_string()]);
        while builder.step(64) != oom_spell::BuildProgress::Complete {}
        let engine = builder.finish().unwrap();
        while session.diagnostics_pending() {
            assert!(session.spell_tick(&engine, 5));
        }
        assert_eq!(session.diagnostics().len(), 1);
        let assert_immediately_conservative = |session: &EditorSession| {
            let mut fresh = EditorSession::from_text(&session.document());
            while fresh.diagnostics_pending() {
                assert!(fresh.spell_tick(&engine, 5));
            }
            for diagnostic in session.diagnostics() {
                assert!(
                    fresh.diagnostics().contains(diagnostic),
                    "mutation retained stale diagnostic {diagnostic:?}"
                );
            }
        };
        let assert_derived = |session: &mut EditorSession| {
            assert_eq!(session.live.highlighter().text(), session.document());
            assert_eq!(
                crate::frontmatter::parse_front_matter(&session.document()),
                *session.front_matter()
            );
            while session.diagnostics_pending() {
                assert!(session.spell_tick(&engine, 5));
            }
            let mut fresh = EditorSession::from_text(&session.document());
            while fresh.diagnostics_pending() {
                assert!(fresh.spell_tick(&engine, 5));
            }
            assert_eq!(session.diagnostics(), fresh.diagnostics());
        };

        session.render_layout(20);
        session.handle_key(key('i'));
        session.insert_paste("prefix ");
        assert_immediately_conservative(&session);
        assert_derived(&mut session);
        assert!(session.front_matter().value().is_none());
        session.handle_key(esc());
        session.handle_key(key('u'));
        assert_immediately_conservative(&session);
        assert_derived(&mut session);
        assert_eq!(
            session
                .front_matter()
                .value()
                .and_then(|value| value.get("title")),
            Some(&crate::frontmatter::Value::str("alpha".to_string()))
        );
        session.handle_key(ctrl('r'));
        assert_immediately_conservative(&session);
        assert_derived(&mut session);
        assert!(session.front_matter().value().is_none());
        session.render_layout(20);
        session.handle_key(key('v'));
        session.handle_key(key('w'));
        session.handle_key(key('d'));
        assert_immediately_conservative(&session);
        assert_derived(&mut session);
        session.handle_key(key('p'));
        assert_immediately_conservative(&session);
        assert_derived(&mut session);

        for input in [
            key(':'),
            key('%'),
            key('s'),
            key('/'),
            key('a'),
            key('l'),
            key('p'),
            key('h'),
            key('a'),
            key('/'),
            key('o'),
            key('m'),
            key('e'),
            key('g'),
            key('a'),
            key('/'),
            special(KeyCodeKind::Enter),
        ] {
            session.handle_key(input);
        }
        assert_immediately_conservative(&session);
        assert_derived(&mut session);
        assert!(session.rendered_state.layout_cache.is_none());
        let diagnostic = session
            .diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.source_text == "omega")
            .cloned()
            .expect("substitute must create a spelling diagnostic");
        session.apply_spell_replacement(&diagnostic, "alpha");
        assert_immediately_conservative(&session);
        assert_derived(&mut session);
    }

    #[test]
    fn canonical_cursor_remap_survives_reflow_without_moving_source() {
        let text = "# Heading\n\nalpha beta gamma delta epsilon zeta\n";
        let mut session = EditorSession::from_text(text);
        session.live.jump_to(2, 17);
        let canonical = session.cursor();
        let source_offset = session.live.cursor_byte_offset();

        for width in [12, 40, 9, 24] {
            session.render_layout(width);
            assert_eq!(session.cursor(), canonical);
            let point = session.rendered_state.cursor.point();
            let layout = session
                .rendered_state
                .layout_cache
                .as_ref()
                .expect("rendered layout should be cached");
            let source = layout.lines[point.row].atoms.iter().find_map(|atom| {
                atom.columns
                    .contains(&point.column)
                    .then_some(atom.source.as_ref())
                    .flatten()
            });
            assert!(
                source.is_some_and(|range| range.contains(&source_offset)),
                "width {width} remapped away from canonical byte {source_offset}: {source:?}"
            );
        }
    }

    #[test]
    fn session_metadata_survives_text_mutation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.md");
        std::fs::write(&path, "one\r\ntwo\r\n").unwrap();
        let mut session = EditorSession::open(&path).unwrap();
        session.render_layout(20);
        session.handle_key(key('i'));
        session.insert_paste("changed ");
        assert_eq!(session.path(), Some(path.as_path()));
        assert_eq!(session.line_ending(), LineEnding::CrLf);
        assert!(session.has_final_newline());
        assert!(!session.is_new());
    }

    #[test]
    fn session_save_uses_authoritative_live_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("saved.md");
        let mut session = EditorSession::from_text("old\n");
        session.render_layout(20);
        session.handle_key(key('i'));
        session.insert_paste("new ");
        let expected = session.document();
        session.save(Some(&path), false).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
    }

    #[test]
    fn source_tab_window_indicators_have_no_source_byte() {
        let mut left = EditorSession::from_text("a\tz");
        let (_, rows, _) = left.render_source_with_atoms(Viewport {
            top_line: 0,
            height: 1,
            width: 4,
            wrap: false,
            left_col: 1,
            skip_rows: 0,
        });
        assert_eq!(rows[0][0].source, None);
        assert_eq!(rows[0][1].source, Some(1..2));

        let mut right = EditorSession::from_text("界\tz");
        let (_, rows, _) = right.render_source_with_atoms(Viewport {
            top_line: 0,
            height: 1,
            width: 4,
            wrap: false,
            left_col: 0,
            skip_rows: 0,
        });
        assert_eq!(rows[0].last().unwrap().source, None);
        assert_eq!(rows[0][1].source, Some(3..4));
    }

    #[test]
    fn core_gt_and_g_upper_t_emit_typed_tab_effects() {
        let mut session = EditorSession::from_text("one\n");
        session.render_layout(20);

        assert!(session.handle_key(key('g')).is_empty());
        assert_eq!(session.handle_key(key('t')), vec![Effect::TabNext]);

        assert!(session.handle_key(key('g')).is_empty());
        assert_eq!(session.handle_key(key('T')), vec![Effect::TabPrev]);

        assert!(session.handle_key(key('3')).is_empty());
        assert!(session.handle_key(key('g')).is_empty());
        assert_eq!(
            session.handle_key(key('t')),
            vec![Effect::TabJump {
                one_based: std::num::NonZeroUsize::new(3).unwrap(),
            }]
        );
    }
}

use crate::input::{KeyCode, KeyCodeKind, KeyInput, Modifiers};

// ── Effect ─────────────────────────────────────────────────────────────────

/// Effects emitted by `EditorSession::handle_key`. The host drains these
/// after each key to decide what to render or act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// A save was requested (from `:w`, `:wq`, etc.).
    SaveRequested {
        /// File path, or `None` for the current buffer.
        path: Option<std::path::PathBuf>,
        /// Force save (ignore read-only flag).
        force: bool,
        /// Whether a path argument becomes the buffer's new path.
        /// False means copy-out (`:w {path}`); true means `:saveas`.
        retarget: bool,
        /// Quit after saving.
        then_quit: bool,
    },
    /// A quit was requested (from `:q`, `:q!`, etc.).
    QuitRequested {
        /// Force quit (ignore unsaved changes).
        force: bool,
    },
    /// An open-file was requested (from `:e`, `:e!`, etc.).
    OpenRequested {
        /// File path to open.
        path: std::path::PathBuf,
        /// Force open (ignore unsaved changes).
        force: bool,
    },
    /// Reload the current file from disk, optionally discarding unsaved edits.
    ReloadCurrentRequested {
        /// Whether unsaved edits may be discarded.
        force: bool,
    },
    /// Reload every open tab from disk, discarding unsaved edits only on success.
    ReloadAllRequested,
    /// Markdown and plain-text forms to write to the system clipboard.
    ClipboardWrite(crate::clipboard::ClipboardContent),
    /// Mode changed.
    ModeChanged(Mode),
    /// A status message to display.
    Message {
        /// The message text.
        text: String,
        /// The message severity.
        severity: Severity,
    },
    /// Cursor moved (render-invalidation hint).
    CursorMoved,
    /// Buffer was edited (dirty may have changed).
    Edited,
    /// Enable or disable source-line wrapping.
    SetWrap(bool),
    /// Help was requested through the core command line (`:help`).
    ///
    /// The TUI opens its command palette with the Vim reference section.
    /// Headless hosts may ignore this effect.
    HelpRequested,
    /// A new tab was requested (from `:tabnew {path}`).
    TabNewRequested {
        /// File path to open in the new tab.
        path: std::path::PathBuf,
    },
    /// Close a tab (from `:tabclose` or `:tabclose!`).
    TabCloseRequested {
        /// Tab index to close; `None` = active tab.
        index: Option<usize>,
        /// Force close (discard unsaved changes).
        force: bool,
    },
    /// Switch to the next tab (from `gt`).
    TabNext,
    /// Switch to the previous tab (from `gT`).
    TabPrev,
    /// Jump to a specific tab by 1-based index (from `{count}gt`).
    TabJump {
        /// 1-based tab index.
        one_based: std::num::NonZeroUsize,
    },
    /// Quit all tabs (from `:qa` or `:qa!`).
    QuitAllRequested {
        /// Force quit (discard unsaved changes).
        force: bool,
    },
}

// ── Severity ───────────────────────────────────────────────────────────────

/// Message severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Informational message.
    Info,
    /// Success message.
    Success,
    /// Warning message.
    Warning,
    /// Error message.
    Error,
}

// ── Viewport ───────────────────────────────────────────────────────────────

/// Viewport specification for rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    /// The 0-based index of the first visible line.
    pub top_line: usize,
    /// The height of the viewport in lines.
    pub height: u16,
    /// The width of the viewport in columns.
    pub width: u16,
    /// Whether long source lines wrap into visual rows.
    pub wrap: bool,
    /// Source-window character offset when wrapping is disabled. When this is
    /// nonzero, the left edge indicator replaces the first window character.
    pub left_col: usize,
    /// Visual rows skipped within `top_line` when wrapping is enabled.
    pub skip_rows: usize,
}

// ── VimCore re-export (internal) ──────────────────────────────────────────

use crate::vim::{
    ProjectedBlockRow, ProjectedSelection, ProjectedYank, RangeOperator, Register, UndoMark,
    VimEffect,
};

// ── Document (internal) ───────────────────────────────────────────────────

use crate::document::{Document, LineEnding};
use crate::error::{OpenError, SaveError};
use crate::frontmatter::FrontMatter;
use crate::rendered::nav;
#[cfg(test)]
use crate::rendered::BlockModel;
use crate::rendered::{LayoutBoundary, ModelChange, RenderedCodeFenceRegion, RetainedRows};
use crate::spell::{
    DecorationKind, Diagnostic, DiagnosticDecorationRow, DiagnosticProvider, PositionError,
    TextPosition,
};
use crate::style::{
    LineKind, RenderedCursor, RenderedLayout, RenderedLineRole, RenderedPoint, RenderedSearch,
    RenderedSelection, RenderedSourceAtom, SearchDirection, SelectionShape, SourceDecoration,
    TargetKind,
};
use live_document::{LiveDocument, PendingProjectionChange};
use std::ops::Range;
use unicode_width::UnicodeWidthChar;

const SOURCE_TAB_STOP: usize = 4;

fn source_character_width(character: char, column: usize) -> usize {
    if character == '\t' {
        SOURCE_TAB_STOP - column % SOURCE_TAB_STOP
    } else {
        character.width().unwrap_or(0)
    }
}

/// Display-only source text with one source range for each visible scalar.
/// Expanded tab spaces all retain the range of their original tab byte.
struct SourceDisplayLine {
    styled: crate::style::StyledLine,
    sources: Vec<Range<usize>>,
    source_to_display_char: Vec<usize>,
}

impl SourceDisplayLine {
    fn new(source: &crate::style::StyledLine) -> Self {
        let mut text = String::with_capacity(source.text.len());
        let mut sources = Vec::new();
        let mut source_to_display_char = vec![0];
        let mut column = 0;
        for (byte, character) in source.text.char_indices() {
            let range = byte..byte + character.len_utf8();
            let width = source_character_width(character, column);
            if character == '\t' {
                text.push_str(&" ".repeat(width));
                sources.extend(std::iter::repeat_n(range, width));
            } else {
                text.push(character);
                sources.push(range);
            }
            column += width;
            source_to_display_char.push(sources.len());
        }
        let spans = source
            .spans
            .iter()
            .map(|span| crate::style::Span {
                start_col: source_to_display_char[span.start_col],
                end_col: source_to_display_char[span.end_col],
                style: span.style,
            })
            .collect();
        Self {
            styled: crate::style::StyledLine { text, spans },
            sources,
            source_to_display_char,
        }
    }

    fn display_char(&self, source_char: usize) -> usize {
        self.source_to_display_char
            .get(source_char)
            .copied()
            .unwrap_or(self.sources.len())
    }

    fn atoms(
        &self,
        text: &str,
        display_char_start: usize,
        source_start: usize,
    ) -> Vec<RenderedSourceAtom> {
        let mut atoms: Vec<RenderedSourceAtom> = Vec::new();
        let mut column = 0;
        let end = display_char_start + text.chars().count();
        for (character, range) in text.chars().zip(&self.sources[display_char_start..end]) {
            let source = source_start + range.start..source_start + range.end;
            let width = character.width().unwrap_or(0);
            if width == 0 {
                if let Some(previous) = atoms.last_mut() {
                    if let Some(previous_source) = previous.source.as_mut() {
                        previous_source.end = source.end;
                    }
                }
                continue;
            }
            atoms.push(RenderedSourceAtom {
                columns: column..column + width,
                source: Some(source),
            });
            column += width;
        }
        atoms
    }
}

/// Vim action applied after mapping a rendered cursor to source editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderedExitAction {
    Insert,
    Append,
    InsertLineStart,
    AppendLineEnd,
    OpenBelow,
    OpenAbove,
}

impl RenderedExitAction {
    fn key(self) -> KeyInput {
        let action = match self {
            Self::Insert => 'i',
            Self::Append => 'a',
            Self::InsertLineStart => 'I',
            Self::AppendLineEnd => 'A',
            Self::OpenBelow => 'o',
            Self::OpenAbove => 'O',
        };

        KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(action),
            },
            mods: Modifiers::default(),
        }
    }
}

/// Translate renderer-owned selection geometry into the minimal request the
/// Vim adapter needs. This is the sole boundary between rendered DTOs and the
/// editor-engine wrapper.
fn project_selection_for_vim(selection: RenderedSelection) -> ProjectedSelection {
    match selection.shape {
        SelectionShape::Character => ProjectedSelection::Character {
            ranges: selection.source_ranges,
        },
        SelectionShape::Line => ProjectedSelection::Line {
            ranges: selection.source_ranges,
        },
        SelectionShape::Block => ProjectedSelection::Block {
            width: selection.block_width.unwrap_or_default(),
            rows: selection
                .rows
                .into_iter()
                .map(|row| ProjectedBlockRow {
                    selected_width: row
                        .columns
                        .first()
                        .map_or(0, |columns| columns.end.saturating_sub(columns.start)),
                    ranges: row.source_ranges,
                })
                .collect(),
        },
    }
}

fn prepare_fenced_code_yank_selection(
    mut selection: RenderedSelection,
    layout: &RenderedLayout,
    document: &str,
    fence_regions: &[RenderedCodeFenceRegion],
) -> RenderedSelection {
    if selection.shape == SelectionShape::Block {
        return selection;
    }

    let selected_rows = selection.anchor.row.min(selection.active.row)
        ..selection
            .anchor
            .row
            .max(selection.active.row)
            .saturating_add(1);
    if selection.shape == SelectionShape::Line
        && fence_regions.iter().any(|region| {
            region.rows.start < selected_rows.start && selected_rows.end < region.rows.end
        })
    {
        selection.source_ranges = layout.lines[selected_rows.clone()]
            .iter()
            .filter(|line| line.role == RenderedLineRole::CodeFence)
            .map(|line| {
                let mut source = line.source.clone();
                if document.as_bytes().get(source.end) == Some(&b'\n') {
                    source.end += 1;
                }
                source
            })
            .collect();
    }
    let covered: Vec<_> = fence_regions
        .iter()
        .filter(|region| {
            selected_rows.start <= region.rows.start && region.rows.end <= selected_rows.end
        })
        .cloned()
        .collect();
    if selection.shape == SelectionShape::Line {
        for region in fence_regions {
            if selected_rows.start == region.rows.start {
                selection
                    .source_ranges
                    .retain(|range| region.source.start < range.end);
                for range in &mut selection.source_ranges {
                    range.start = range.start.max(region.source.start);
                }
            }
            if selected_rows.end == region.rows.end {
                selection
                    .source_ranges
                    .retain(|range| range.start < region.source.end);
                for range in &mut selection.source_ranges {
                    range.end = range.end.min(region.source.end);
                }
            }
        }
    }
    selection
        .source_ranges
        .extend(covered.into_iter().map(|region| region.source));
    selection
        .source_ranges
        .retain(|range| range.start < range.end);
    selection
        .source_ranges
        .sort_by_key(|range| (range.start, range.end));
    let mut normalized: Vec<Range<usize>> = Vec::with_capacity(selection.source_ranges.len());
    for range in selection.source_ranges.drain(..) {
        if let Some(previous) = normalized.last_mut() {
            if range.start <= previous.end {
                previous.end = previous.end.max(range.end);
                continue;
            }
        }
        normalized.push(range);
    }
    selection.source_ranges = normalized;
    selection
}

// ── RenderedState ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectionEndpoint {
    point: RenderedPoint,
    source: (usize, usize),
    atom: Option<Range<usize>>,
    line: Option<(Range<usize>, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectionKind {
    Character { ranges: Vec<Range<usize>> },
    Line,
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YankPublication {
    Configured,
    PlainText,
}

impl SelectionKind {
    fn shape(&self) -> SelectionShape {
        match self {
            Self::Character { .. } => SelectionShape::Character,
            Self::Line => SelectionShape::Line,
            Self::Block => SelectionShape::Block,
        }
    }

    fn from_shape(shape: SelectionShape) -> Self {
        match shape {
            SelectionShape::Character => Self::Character { ranges: Vec::new() },
            SelectionShape::Line => Self::Line,
            SelectionShape::Block => Self::Block,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveSelection {
    anchor: SelectionEndpoint,
    active: SelectionEndpoint,
    kind: SelectionKind,
}

impl ActiveSelection {
    fn swap_endpoints(&mut self) {
        std::mem::swap(&mut self.anchor, &mut self.active);
    }

    fn switch_kind(&mut self, shape: SelectionShape) {
        self.kind = SelectionKind::from_shape(shape);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionMode {
    CoreDriven,
    Select(ActiveSelection),
    Command(CommandPrompt),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct CommandPrompt {
    text: String,
    traversal: CommandTraversal,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum CommandTraversal {
    #[default]
    Draft,
    Recalling {
        draft: String,
        index: usize,
    },
}

impl CommandPrompt {
    fn detach(&mut self) {
        self.traversal = CommandTraversal::Draft;
    }

    fn up(&mut self, history: &CommandHistory) {
        let entries = history.0.borrow();
        if entries.is_empty() {
            return;
        }
        let (draft, index) = match &self.traversal {
            CommandTraversal::Draft => (self.text.clone(), entries.len() - 1),
            CommandTraversal::Recalling { draft, index } => {
                (draft.clone(), index.saturating_sub(1))
            }
        };
        self.text.clone_from(&entries[index]);
        self.traversal = CommandTraversal::Recalling { draft, index };
    }

    fn down(&mut self, history: &CommandHistory) {
        let CommandTraversal::Recalling { draft, index } = &self.traversal else {
            return;
        };
        let entries = history.0.borrow();
        if *index + 1 < entries.len() {
            let next = *index + 1;
            self.text.clone_from(&entries[next]);
            self.traversal = CommandTraversal::Recalling {
                draft: draft.clone(),
                index: next,
            };
        } else {
            self.text.clone_from(draft);
            self.traversal = CommandTraversal::Draft;
        }
    }
}

/// Process-local, bounded ex-command history shared by editor sessions.
#[derive(Debug, Clone, Default)]
pub struct CommandHistory(std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<String>>>);

impl CommandHistory {
    /// Create an empty history handle.
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&self, command: String) {
        let mut entries = self.0.borrow_mut();
        if entries.len() == 10 {
            entries.pop_front();
        }
        entries.push_back(command);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum RegisterInput {
    #[default]
    Default,
    AwaitingName,
    Selected(Register),
}

impl RegisterInput {
    fn select(&mut self, selector: char) {
        *self = EditorSession::rendered_register(selector)
            .map(Self::Selected)
            .unwrap_or(Self::Default);
    }

    fn take(&mut self) -> Register {
        match std::mem::take(self) {
            Self::Selected(register) => register,
            Self::Default | Self::AwaitingName => Register::Unnamed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RenderedSearchState {
    Inactive {
        last: Option<RenderedSearch>,
    },
    Prompt {
        draft: RenderedSearch,
        origin: RenderedCursor,
        last: Option<RenderedSearch>,
    },
}

impl Default for RenderedSearchState {
    fn default() -> Self {
        Self::Inactive { last: None }
    }
}

impl RenderedSearchState {
    fn current(&self) -> Option<&RenderedSearch> {
        match self {
            Self::Inactive { last } => last.as_ref(),
            Self::Prompt { draft, .. } => Some(draft),
        }
    }

    fn prompt(&self) -> Option<(&RenderedSearch, &RenderedCursor)> {
        match self {
            Self::Prompt { draft, origin, .. } => Some((draft, origin)),
            Self::Inactive { .. } => None,
        }
    }

    fn begin(&mut self, draft: RenderedSearch, origin: RenderedCursor) {
        let last = match std::mem::take(self) {
            Self::Inactive { last } | Self::Prompt { last, .. } => last,
        };
        *self = Self::Prompt {
            draft,
            origin,
            last,
        };
    }

    fn update_draft(&mut self, update: impl FnOnce(&mut RenderedSearch)) {
        if let Self::Prompt { draft, .. } = self {
            update(draft);
        }
    }

    fn submit(&mut self) {
        let replacement = match std::mem::take(self) {
            Self::Prompt { draft, .. } => Some(draft),
            Self::Inactive { last } => last,
        };
        *self = Self::Inactive { last: replacement };
    }

    fn cancel(&mut self) {
        let last = match std::mem::take(self) {
            Self::Prompt { last, .. } | Self::Inactive { last } => last,
        };
        *self = Self::Inactive { last };
    }

    fn replace_last(&mut self, search: RenderedSearch) {
        *self = Self::Inactive { last: Some(search) };
    }

    fn clear(&mut self) {
        *self = Self::Inactive { last: None };
    }
}

/// Persistent state shared by rendered Normal, Select, and Command.
///
/// Holds a cached layout, cursor position, search state, and front-matter
/// panel collapse state. The layout is invalidated on edits.
#[cfg(test)]
#[derive(Default)]
struct ProjectionWork {
    rebuilt_blocks: usize,
    reused_blocks: usize,
    rebuilt_rows: usize,
    reused_rows: usize,
}

struct RenderedState {
    /// Cached rendered layout (None = needs rebuild).
    layout_cache: Option<RenderedLayout>,
    /// One retained row store for bounded viewport drawing.
    row_cache: Option<RetainedRows>,
    /// Ownership boundaries for moving a flat build into retained rows.
    layout_boundaries: Vec<LayoutBoundary>,
    /// Full source spans paired with rendered fenced-code row intervals.
    code_fence_regions: Vec<RenderedCodeFenceRegion>,
    /// The width used when the layout was last built.
    last_width: u16,
    /// Current cursor position in rendered coordinates.
    cursor: RenderedCursor,
    /// Mutually exclusive register-prefix input state, shared by Normal put
    /// and Select operators.
    register_input: RegisterInput,
    /// Submitted rendered search or an atomic prompt with fixed origin and
    /// preserved prior history.
    search: RenderedSearchState,
    /// Whether the front-matter panel is collapsed.
    fm_collapsed: bool,
    /// Accumulated numeric count for navigation commands.
    count: usize,
    /// Whether the first `g` of rendered `gg` is pending.
    pending_g: bool,
    /// First bracket of a rendered `[[` or `]]` heading motion.
    pending_heading_bracket: Option<char>,
    /// Actual rendered layout builds, exposed only to regression tests.
    #[cfg(test)]
    layout_builds: usize,
    #[cfg(test)]
    last_work: ProjectionWork,
    #[cfg(test)]
    last_publish_profile_ns: [u128; 3],
    #[cfg(test)]
    last_publish_used_retained: bool,
    #[cfg(test)]
    last_select_operator_profile_ns: [u128; 6],
}

impl RenderedState {
    fn new() -> Self {
        Self {
            layout_cache: None,
            row_cache: None,
            layout_boundaries: Vec::new(),
            code_fence_regions: Vec::new(),
            last_width: 0,
            cursor: RenderedCursor::new(0),
            register_input: RegisterInput::Default,
            search: RenderedSearchState::default(),
            fm_collapsed: false,
            count: 0,
            pending_g: false,
            pending_heading_bracket: None,
            #[cfg(test)]
            layout_builds: 0,
            #[cfg(test)]
            last_work: ProjectionWork::default(),
            #[cfg(test)]
            last_publish_profile_ns: [0; 3],
            #[cfg(test)]
            last_publish_used_retained: false,
            #[cfg(test)]
            last_select_operator_profile_ns: [0; 6],
        }
    }

    fn needs_layout(&self, width: u16) -> bool {
        self.layout_cache.is_none() || self.last_width != width
    }

    /// Invalidate the layout cache.
    fn invalidate(&mut self) {
        self.layout_cache = None;
        self.row_cache = None;
        self.layout_boundaries.clear();
        self.code_fence_regions.clear();
    }
}

// ── EditorSession ──────────────────────────────────────────────────────────

/// The core editing session. This is the public façade through which a host
/// feeds keys, drains effects, and queries state.
///
/// See architecture §6 for the full API contract.
pub struct EditorSession {
    /// Canonical live text plus synchronously-derived caches.
    live: LiveDocument,
    /// Session-owned modes; Normal and Insert are always derived from Vim.
    session_mode: SessionMode,
    /// Dirty generation at last save.
    save_point: SaveBaseline,
    /// History shared with other sessions in this editor process.
    command_history: CommandHistory,
    /// The document model — text, path, front matter, I/O state.
    document: Document,
    /// Persistent rendered navigation and Select state.
    rendered_state: RenderedState,
}

enum SaveBaseline {
    Confirmed(UndoMark),
    CommittedUncertain(UndoMark),
}

impl EditorSession {
    /// Create a new session from initial text. Starts in Normal mode.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::EditorSession;
    ///
    /// let session = EditorSession::from_text("# Hello\n\nWorld\n");
    /// assert_eq!(session.mode(), oom_edit_core::Mode::Normal);
    /// assert_eq!(session.line_count(), 4);
    /// ```
    pub fn from_text(text: &str) -> Self {
        let (normalized, document) = Document::from_text(text).into_parts();
        let mut live = LiveDocument::new(&normalized);
        let save_point = live.save_point();
        Self {
            live,
            session_mode: SessionMode::CoreDriven,
            save_point: SaveBaseline::Confirmed(save_point),
            command_history: CommandHistory::new(),
            document,
            rendered_state: RenderedState::new(),
        }
    }

    /// Open a session from a file path.
    ///
    /// If the file does not exist, creates a new-buffer session with empty
    /// text and the path retained (FR-6.10 / new-file semantics).
    ///
    /// Per FR-5.1: invalid UTF-8 is refused with the byte offset of the
    /// first bad byte.
    pub fn open(path: &std::path::Path) -> Result<Self, OpenError> {
        let (text, document) = Document::open(path)?.into_parts();
        Ok(Self::from_opened_document(text, document))
    }

    /// Open an existing file; missing files are errors rather than new buffers.
    pub fn open_existing(path: &std::path::Path) -> Result<Self, OpenError> {
        let (text, document) = Document::open_existing(path)?.into_parts();
        Ok(Self::from_opened_document(text, document))
    }

    fn from_opened_document(text: String, document: Document) -> Self {
        let mut live = LiveDocument::new(&text);
        let save_point = live.save_point();
        Self {
            live,
            session_mode: SessionMode::CoreDriven,
            save_point: SaveBaseline::Confirmed(save_point),
            command_history: CommandHistory::new(),
            document,
            rendered_state: RenderedState::new(),
        }
    }

    /// Save the document to its path (or the given override path).
    ///
    /// `force: true` bypasses external-modification detection (FR-5.7).
    ///
    /// Returns `SaveError::ExternallyModified` if the file was externally
    /// modified and `force` is `false` (FR-5.7).
    pub fn save(&mut self, path: Option<&std::path::Path>, force: bool) -> Result<(), SaveError> {
        self.save_with_operations(path, force, &mut crate::document::FileSystemAtomicSave)
    }

    /// Save through the normal atomic protocol with an injected boundary observer.
    pub fn save_with_observer(
        &mut self,
        path: Option<&std::path::Path>,
        force: bool,
        observer: &mut dyn crate::SaveObserver,
    ) -> Result<(), SaveError> {
        let target = path
            .or_else(|| self.path())
            .ok_or_else(|| SaveError::Io(std::io::Error::other("document has no path")))?
            .to_path_buf();
        self.save_with_operations(
            path,
            force,
            &mut crate::document::ObservedAtomicSave { observer, target },
        )
    }

    /// Overwrite or recreate only the exact version presented to the user.
    /// The version is checked again immediately before atomic replacement.
    pub fn save_if_version(
        &mut self,
        path: Option<&std::path::Path>,
        expected: &crate::DiskVersion,
    ) -> Result<(), SaveError> {
        let text = self.live.text();
        let result = self.document.save_with_text_expected_using(
            &text,
            path,
            true,
            Some(expected),
            &mut crate::document::FileSystemAtomicSave,
        );
        if let Err(error) = result {
            if matches!(error, SaveError::CommittedUncertain(_)) {
                let mark = match self.save_point {
                    SaveBaseline::Confirmed(mark) | SaveBaseline::CommittedUncertain(mark) => mark,
                };
                self.save_point = SaveBaseline::CommittedUncertain(mark);
            }
            return Err(error);
        }
        self.save_point = SaveBaseline::Confirmed(self.live.save_point());
        Ok(())
    }

    fn save_with_operations<O: crate::document::AtomicSaveOperations>(
        &mut self,
        path: Option<&std::path::Path>,
        force: bool,
        operations: &mut O,
    ) -> Result<(), SaveError> {
        // Get the current text from the vim buffer
        let text = self.live.text();
        // Save using the document's I/O logic, passing the vim buffer text
        if let Err(error) = self
            .document
            .save_with_text_using(&text, path, force, operations)
        {
            if matches!(error, SaveError::CommittedUncertain(_)) {
                let mark = match self.save_point {
                    SaveBaseline::Confirmed(mark) | SaveBaseline::CommittedUncertain(mark) => mark,
                };
                self.save_point = SaveBaseline::CommittedUncertain(mark);
            }
            return Err(error);
        }
        // The vim engine owns the authoritative dirty generation. Capture it
        // once after the save succeeds.
        let mark = self.live.save_point();
        self.save_point = SaveBaseline::Confirmed(mark);
        Ok(())
    }

    /// Atomically save a copy without retargeting the buffer or clearing its
    /// dirty state (`:w {path}`).
    pub fn save_copy(&self, path: &std::path::Path) -> Result<(), SaveError> {
        self.document.save_copy_with_text(&self.live.text(), path)
    }

    /// Save a copy through the atomic protocol with an injected boundary observer.
    pub fn save_copy_with_observer(
        &self,
        path: &std::path::Path,
        observer: &mut dyn crate::SaveObserver,
    ) -> Result<(), SaveError> {
        self.document.save_copy_with_text_using(
            &self.live.text(),
            path,
            &mut crate::document::ObservedAtomicSave {
                observer,
                target: path.to_path_buf(),
            },
        )
    }

    /// Content-validate the backing path and return its versioned state.
    pub fn disk_state(&self) -> crate::document::DiskState {
        self.document.disk_state()
    }

    /// Probe only metadata for routine polling; Unchanged is not a content guarantee.
    pub fn disk_hint(&self) -> crate::document::DiskHint {
        self.document.disk_hint()
    }

    /// Accept the displayed modified version for a later non-forced save.
    pub fn acknowledge_keep_mine(
        &mut self,
        expected: &crate::document::DiskVersion,
    ) -> Result<(), crate::error::DiskDecisionError> {
        self.document.acknowledge_keep_mine(expected)
    }

    /// Authorize recreation of this exact missing-path version.
    pub fn authorize_recreation(
        &mut self,
        expected: &crate::document::DiskVersion,
    ) -> Result<(), crate::error::DiskDecisionError> {
        self.document.authorize_recreation(expected)
    }

    /// Reload a candidate only if its content-validated version is still current.
    pub fn reload_from_disk(
        &mut self,
        expected: &crate::document::DiskVersion,
    ) -> Result<(), crate::error::ReloadError> {
        let candidate = self.document.prepare_reload(expected)?;
        if candidate.text != self.live.text_ref() {
            let cursor = self.live.cursor();
            // Release rows for the obsolete text before allocating replacement
            // source analysis; reload will rebuild the rendered view on demand.
            self.rendered_state = RenderedState::new();
            let _outcome = self.live.reload(&candidate.text, cursor);
            self.session_mode = SessionMode::CoreDriven;
        }
        self.document.commit_reload(&candidate);
        self.save_point = SaveBaseline::Confirmed(self.live.save_point());
        self.rendered_state.invalidate();
        Ok(())
    }

    /// Follow a validated rename without writing or changing editor state.
    pub fn retarget(
        &mut self,
        destination: &std::path::Path,
        expected: &crate::document::DiskVersion,
    ) -> Result<(), crate::error::RetargetError> {
        self.document.retarget(destination, expected)
    }

    /// Capture source and destination versions before a host-managed move.
    pub fn prepare_retarget(
        &self,
        destination: &std::path::Path,
        expected_source: &crate::DiskVersion,
    ) -> Result<crate::RetargetPreparation, crate::RetargetError> {
        self.document.prepare_retarget(destination, expected_source)
    }

    /// Validate moved identity without changing the session's binding.
    pub fn validate_retarget(
        &self,
        preparation: &crate::RetargetPreparation,
    ) -> Result<crate::RetargetBinding, crate::RetargetError> {
        self.document.validate_retarget(preparation)
    }

    /// Check that an IO-free binding still addresses this document generation.
    pub fn can_commit_retarget(&self, binding: &crate::RetargetBinding) -> bool {
        self.document.can_commit_retarget(binding)
    }

    /// Apply a validated binding without file IO or changes to editor state.
    pub fn commit_retarget(
        &mut self,
        binding: crate::RetargetBinding,
    ) -> Result<(), crate::RetargetError> {
        self.document.commit_retarget(binding)
    }

    /// Handle a key input. Returns zero or more effects.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::{EditorSession, KeyInput, KeyCode, KeyCodeKind, Modifiers};
    ///
    /// let mut session = EditorSession::from_text("hello");
    /// let key = KeyInput {
    ///     code: KeyCode { kind: KeyCodeKind::Char('i') },
    ///     mods: Modifiers::default(),
    /// };
    /// let effects = session.handle_key(key);
    /// assert!(effects.iter().any(|e| matches!(e, oom_edit_core::Effect::ModeChanged(_))));
    /// assert_eq!(session.mode(), oom_edit_core::Mode::Insert);
    /// ```
    pub fn handle_key(&mut self, key: KeyInput) -> Vec<Effect> {
        // Unsupported terminal keys are consumed without reaching any mode
        // handler or the Vim engine, where fallback mappings could otherwise
        // cause edits, cursor movement, mode changes, or command effects.
        if key.code.kind == KeyCodeKind::Noop {
            return Vec::new();
        }

        match self.mode() {
            Mode::Normal => self.handle_rendered_normal_key(key),
            Mode::Select => self.handle_rendered_select_key(key),
            Mode::Insert => self.handle_insert_key(key),
            Mode::Command => self.handle_command_mode_key(key),
        }
    }

    /// Return the current mode.
    pub fn mode(&self) -> Mode {
        match self.session_mode {
            SessionMode::CoreDriven => {
                if self.live.mode() == crate::vim::Mode::Insert {
                    Mode::Insert
                } else {
                    Mode::Normal
                }
            }
            SessionMode::Select(_) => Mode::Select,
            SessionMode::Command(_) => Mode::Command,
        }
    }

    /// Clear an in-flight engine prefix when its tab loses active ownership.
    /// Mode, prompt, selection, text and undo remain unchanged.
    pub fn clear_pending_input(&mut self) {
        self.live.clear_pending_input();
        self.rendered_state.register_input = RegisterInput::Default;
        self.rendered_state.count = 0;
        self.rendered_state.pending_g = false;
        self.rendered_state.pending_heading_bracket = None;
    }

    /// Whether a buffer-local prefix is waiting for another key.
    pub fn has_pending_input(&self) -> bool {
        self.live.has_pending_input()
            || self.rendered_state.register_input != RegisterInput::Default
            || self.rendered_state.count != 0
            || self.rendered_state.pending_g
            || self.rendered_state.pending_heading_bracket.is_some()
    }

    /// Handle a key using the host's monotonic elapsed time for engine chords.
    /// The timeout clock is restored after this dispatch.
    pub fn handle_key_at(
        &mut self,
        key: KeyInput,
        host_elapsed: std::time::Duration,
    ) -> Vec<Effect> {
        self.live.set_host_time(Some(host_elapsed));
        let effects = self.handle_key(key);
        self.live.set_host_time(None);
        effects
    }

    /// Return the full document text.
    pub fn document(&self) -> String {
        self.live.text()
    }

    /// Enable or disable spell checking for this session.
    pub fn set_spell_enabled(&mut self, enabled: bool) {
        self.live.set_spell_enabled(enabled);
    }

    /// Return whether spell checking is enabled for this session.
    pub fn spell_enabled(&self) -> bool {
        self.live.spell_enabled()
    }

    /// Advance spell scanning by at most `max_bytes` source bytes.
    ///
    /// Returns `true` when state or scan progress changed. Disabled sessions,
    /// zero budgets, and already-clean sessions return `false`.
    pub fn spell_tick(&mut self, engine: &oom_spell::SpellEngine, max_bytes: usize) -> bool {
        self.live.spell_tick(engine, max_bytes)
    }

    /// Borrow the sorted, conservatively valid diagnostics visible to hosts.
    ///
    /// Disabled sessions expose an empty slice even while invalid scan state
    /// is retained for a later re-enable.
    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.live.diagnostics()
    }

    /// Return whether enabled spelling has incomplete scan work.
    pub fn diagnostics_pending(&self) -> bool {
        self.live.diagnostics_pending()
    }

    /// Return the diagnostic containing the canonical cursor byte offset.
    ///
    /// Diagnostic ranges are half-open: a cursor exactly at `range.end` is
    /// outside that diagnostic.
    pub fn diagnostic_at_cursor(&self) -> Option<&Diagnostic> {
        let offset = self.live.cursor_byte_offset();
        self.diagnostics()
            .iter()
            .find(|diagnostic| diagnostic.range.contains(&offset))
    }

    /// Project visible diagnostics into rendered display-cell intervals.
    ///
    /// Establish the host width with [`Self::render_layout`] or
    /// [`Self::rendered_viewport`] first. Rows outside `visible` are clipped,
    /// and source-less presentation atoms are never included.
    pub fn diagnostic_decoration_rows(
        &mut self,
        visible: Range<usize>,
    ) -> Vec<DiagnosticDecorationRow> {
        let lines = if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
            let start = visible.start.min(layout.lines.len());
            let end = visible.end.min(layout.lines.len()).max(start);
            layout.lines[start..end].to_vec()
        } else if let Some(rows) = self.rendered_state.row_cache.as_mut() {
            (visible.start..visible.end.min(rows.row_count()))
                .filter_map(|row| {
                    rows.row_current(
                        row,
                        self.live.rendered_model(),
                        self.live.text_ref(),
                        self.live.highlighter(),
                        self.rendered_state.last_width,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            return Vec::new();
        };
        let start = visible.start;
        let end = start + lines.len();
        if start >= end {
            return Vec::new();
        }
        let source_intervals =
            Self::visible_source_intervals(lines.iter().map(|line| line.atoms.as_slice()));
        let diagnostics = self.diagnostics();
        Self::visible_diagnostic_indices(diagnostics, &source_intervals)
            .into_iter()
            .flat_map(|index| {
                let diagnostic = &diagnostics[index];
                let kind = DecorationKind::Diagnostic {
                    provider: diagnostic.provider,
                    severity: diagnostic.severity,
                };
                lines.iter().enumerate().flat_map(move |(offset, line)| {
                    nav::project_atom_intervals(&diagnostic.range, &line.atoms)
                        .into_iter()
                        .map(move |columns| DiagnosticDecorationRow {
                            row: start + offset,
                            columns,
                            kind,
                        })
                })
            })
            .collect()
    }

    /// Return deterministic spelling suggestions for a current diagnostic.
    ///
    /// Stale diagnostics and diagnostics from another provider return an
    /// empty list.
    pub fn spell_suggestions(
        &self,
        engine: &oom_spell::SpellEngine,
        diagnostic: &Diagnostic,
        max: usize,
    ) -> Vec<String> {
        if diagnostic.provider != DiagnosticProvider::Spell
            || !self
                .diagnostics()
                .iter()
                .any(|current| current == diagnostic)
        {
            return Vec::new();
        }
        let Some(word) = self.live.text_ref().get(diagnostic.range.clone()) else {
            return Vec::new();
        };
        engine.suggest(word, max)
    }

    /// Apply one exact spelling correction after revalidating diagnostic identity and text.
    ///
    /// A stale diagnostic emits a warning and never changes the document.
    pub fn apply_spell_replacement(
        &mut self,
        diagnostic: &Diagnostic,
        replacement: &str,
    ) -> Vec<Effect> {
        let current = self
            .diagnostics()
            .iter()
            .any(|candidate| candidate == diagnostic);
        let text_matches = self
            .live
            .text_ref()
            .get(diagnostic.range.clone())
            .is_some_and(|text| text == diagnostic.source_text);
        if diagnostic.provider != DiagnosticProvider::Spell || !current || !text_matches {
            return vec![Effect::Message {
                text: "Spelling diagnostic is stale; no replacement was applied".to_string(),
                severity: Severity::Warning,
            }];
        }
        let Some(outcome) = self
            .live
            .replace_range(diagnostic.range.clone(), replacement)
        else {
            return vec![Effect::Message {
                text: "Spelling diagnostic range is invalid; no replacement was applied"
                    .to_string(),
                severity: Severity::Warning,
            }];
        };
        self.translate_vim_effects(outcome)
    }

    /// Return an exact owned slice for a validated UTF-8 byte range.
    ///
    /// Reversed, out-of-bounds, and mid-scalar ranges return `None`; logical
    /// EOF (`len..len`) is valid.
    pub fn text_for_range(&self, range: Range<usize>) -> Option<String> {
        if range.start > range.end {
            return None;
        }
        self.live.text_ref().get(range).map(ToOwned::to_owned)
    }

    /// Map a UTF-8 byte offset to a zero-based line and Unicode-scalar column.
    ///
    /// Logical EOF is valid. Offsets beyond EOF or in the middle of a scalar
    /// return `None`.
    pub fn position_for_offset(&self, offset: usize) -> Option<TextPosition> {
        let text = self.live.text_ref();
        if offset > text.len() || !text.is_char_boundary(offset) {
            return None;
        }
        let (line, column) = self.live.position_for_byte_offset(offset);
        Some(TextPosition { line, column })
    }

    /// Atomically move both canonical and rendered cursors to a source offset.
    pub fn jump_to_offset(&mut self, offset: usize) -> Result<Vec<Effect>, PositionError> {
        let text = self.live.text_ref();
        if offset > text.len() {
            return Err(PositionError::OutOfBounds);
        }
        if !text.is_char_boundary(offset) {
            return Err(PositionError::NotCharBoundary);
        }
        let position = self
            .position_for_offset(offset)
            .expect("validated source offset must have a position");
        self.live.jump_to(position.line, position.column);
        self.remap_active_cursor_from_canonical();
        self.refresh_character_selection();
        Ok(vec![Effect::CursorMoved])
    }

    /// Move the cursor to the closest source-backed rendered atom.
    pub fn move_to_rendered_point(&mut self, point: RenderedPoint) -> Vec<Effect> {
        let candidate = if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
            nav::source_backed_point(point, layout).and_then(|point| {
                nav::source_for_point(point, layout).map(|source| (point, source))
            })
        } else if let Some(rows) = self.rendered_state.row_cache.as_mut() {
            rows.source_backed_point(
                point,
                self.live.rendered_model(),
                self.live.text_ref(),
                self.live.highlighter(),
                self.rendered_state.last_width,
            )
        } else {
            None
        };
        let Some((point, source)) = candidate else {
            return Vec::new();
        };
        let offset = source.start;
        let position = self.live.position_for_byte_offset(offset);
        self.live.jump_to(position.0, position.1);
        self.rendered_state.cursor = RenderedCursor::at(point);
        self.refresh_character_selection();
        vec![Effect::CursorMoved]
    }

    /// Start or update a rendered character selection between display cells.
    pub fn select_rendered_points(
        &mut self,
        anchor: RenderedPoint,
        active: RenderedPoint,
    ) -> Vec<Effect> {
        if (self.rendered_state.layout_cache.is_none() && self.rendered_state.row_cache.is_none())
            || matches!(self.mode(), Mode::Insert | Mode::Command)
        {
            return Vec::new();
        }
        if self.mode() == Mode::Select {
            self.finish_select(Mode::Normal, Vec::new());
        }
        let mut effects = self.move_to_rendered_point(anchor);
        if effects.is_empty() {
            return effects;
        }
        effects.extend(self.enter_select(SelectionShape::Character));
        effects.extend(self.move_to_rendered_point(active));
        effects
    }

    /// Start a rendered character selection from two source byte offsets.
    /// This is used when a drag begins in the source Insert view.
    pub fn select_source_offsets(
        &mut self,
        anchor: usize,
        active: usize,
        rendered_width: u16,
    ) -> Result<Vec<Effect>, PositionError> {
        for offset in [anchor, active] {
            if offset > self.live.text_ref().len() {
                return Err(PositionError::OutOfBounds);
            }
            if !self.live.text_ref().is_char_boundary(offset) {
                return Err(PositionError::NotCharBoundary);
            }
        }
        let mut effects = Vec::new();
        if self.mode() == Mode::Insert {
            effects.extend(self.handle_key(KeyInput {
                code: KeyCode {
                    kind: KeyCodeKind::Esc,
                },
                mods: Modifiers::default(),
            }));
        }
        self.render_layout(rendered_width);
        effects.extend(self.jump_to_offset(anchor)?);
        let anchor_point = self.rendered_cursor();
        effects.extend(self.jump_to_offset(active)?);
        let active_point = self.rendered_cursor();
        effects.extend(self.select_rendered_points(anchor_point, active_point));
        Ok(effects)
    }

    /// Return the current file path, if this buffer has one.
    pub fn path(&self) -> Option<&std::path::Path> {
        self.document.path()
    }

    /// Whether this buffer targets a path that has not yet been saved.
    pub fn is_new(&self) -> bool {
        self.document.is_new()
    }

    /// Line-ending policy retained for serialization.
    pub fn line_ending(&self) -> LineEnding {
        self.document.line_ending()
    }

    /// Whether serialization retains a final newline.
    pub fn has_final_newline(&self) -> bool {
        self.document.has_final_newline()
    }

    /// Parsed front matter derived from the current unsaved live text.
    pub fn front_matter(&self) -> &FrontMatter {
        self.live.front_matter()
    }

    /// Prepend the built-in YAML front-matter template when none is present.
    ///
    /// The insertion is one undoable mutation. Existing YAML or TOML front
    /// matter, including malformed leading blocks, leaves the session
    /// unchanged and returns no effects.
    pub fn insert_default_front_matter(&mut self) -> Vec<Effect> {
        const TEMPLATE: &str = "---\ntitle: \"\"\n---\n\n";
        if self.mode() != Mode::Normal || self.live.front_matter().is_some() {
            return Vec::new();
        }

        let cursor_offset = self.live.cursor_byte_offset();
        let Some(outcome) = self.live.replace_range(0..0, TEMPLATE) else {
            return Vec::new();
        };
        let shifted = self
            .live
            .position_for_byte_offset(cursor_offset.saturating_add(TEMPLATE.len()));
        self.live.jump_to(shifted.0, shifted.1);
        self.translate_vim_effects(outcome)
    }

    /// Return cursor position as `(line, col)` — 0-based.
    pub fn cursor(&self) -> (usize, usize) {
        self.live.cursor()
    }

    /// Return the unprefixed command-line text, or `None` outside Command mode.
    pub fn command_line(&self) -> Option<String> {
        match &self.session_mode {
            SessionMode::Command(prompt) => Some(prompt.text.clone()),
            _ => None,
        }
    }

    /// Enter Command mode with editable ex text without submitting it.
    ///
    /// The text is unprefixed and must be one printable command line. Hosts can
    /// use this to hand off a selected command reference to the core grammar.
    pub fn open_command_prompt(&mut self, prefill: &str) -> Vec<Effect> {
        if !matches!(self.mode(), Mode::Normal | Mode::Select)
            || prefill.trim().is_empty()
            || prefill.starts_with(':')
            || prefill.chars().any(char::is_control)
        {
            return vec![Effect::Message {
                text: "Cannot open a command prompt with that text".to_string(),
                severity: Severity::Warning,
            }];
        }
        self.rendered_state.search.cancel();
        self.session_mode = SessionMode::Command(CommandPrompt {
            text: prefill.to_string(),
            traversal: CommandTraversal::Draft,
        });
        vec![Effect::ModeChanged(Mode::Command)]
    }

    /// Share an in-memory ex-command history with other editor sessions.
    pub fn set_command_history(&mut self, history: CommandHistory) {
        self.command_history = history;
    }

    /// Return the active rendered-search prompt, including `/` or `?` prefix.
    ///
    /// Submitted or cancelled prompts return `None` even though the last
    /// search remains available for `n`/`N`.
    pub fn rendered_search_prompt(&self) -> Option<String> {
        let (search, _) = self.rendered_state.search.prompt()?;
        let prefix = match search.last_direction {
            SearchDirection::Forward => '/',
            SearchDirection::Backward => '?',
        };
        Some(format!("{prefix}{}", search.pattern))
    }

    /// Return the rendered cursor position.
    pub fn rendered_cursor(&self) -> RenderedPoint {
        self.rendered_state.cursor.point()
    }

    /// Return the cached rendered layout, if a host width has been supplied.
    pub fn rendered_layout(&self) -> Option<&RenderedLayout> {
        self.rendered_state.layout_cache.as_ref()
    }

    /// Return the rendered layout, building it at the supplied text width.
    pub fn rendered_layout_mut(&mut self, width: u16) -> &RenderedLayout {
        self.render_layout(width)
    }

    /// Return the retained rendered search state.
    pub fn rendered_search(&self) -> Option<&RenderedSearch> {
        self.rendered_state.search.current()
    }

    /// Return renderer-neutral Select metadata, or `None` outside Select.
    pub fn rendered_selection(&self) -> Option<RenderedSelection> {
        let SessionMode::Select(active) = &self.session_mode else {
            return None;
        };
        let lines = if matches!(active.kind, SelectionKind::Line) {
            self.line_selection_lines(active)
        } else {
            self.selection_lines(active.anchor.point, active.active.point)
        };
        Some(nav::project_selection_from_rows(
            active.anchor.point,
            active.active.point,
            active.kind.shape(),
            (active.anchor.source, active.active.source),
            &lines,
            match &active.kind {
                SelectionKind::Character { ranges } => Some(ranges),
                SelectionKind::Line | SelectionKind::Block => None,
            },
            self.live.text_ref(),
        ))
    }

    fn selection_lines(
        &self,
        anchor: RenderedPoint,
        active: RenderedPoint,
    ) -> Vec<(usize, crate::style::RenderedLine)> {
        let first = anchor.row.min(active.row);
        let end = anchor.row.max(active.row).saturating_add(1);
        if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
            return (first..end)
                .filter_map(|row| layout.lines.get(row).cloned().map(|line| (row, line)))
                .collect();
        }
        self.rendered_state
            .row_cache
            .as_ref()
            .map_or_else(Vec::new, |rows| {
                rows.rows_current_range(
                    first..end,
                    self.live.rendered_model(),
                    self.live.highlighter(),
                    self.rendered_state.last_width,
                )
            })
    }

    fn line_selection_lines(
        &self,
        active: &ActiveSelection,
    ) -> Vec<(usize, crate::style::RenderedLine)> {
        let text = self.live.text_ref();
        let selected = nav::physical_lines_for_source_positions(
            active.anchor.source,
            active.active.source,
            text,
        );
        if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
            let mut first = active.anchor.point.row.min(active.active.point.row);
            let mut last = active.anchor.point.row.max(active.active.point.row);
            while first > 0
                && nav::line_row_intersects_source(&layout.lines[first - 1], &selected, text)
            {
                first -= 1;
            }
            while last + 1 < layout.lines.len()
                && nav::line_row_intersects_source(&layout.lines[last + 1], &selected, text)
            {
                last += 1;
            }
            return (first..=last)
                .filter_map(|row| layout.lines.get(row).cloned().map(|line| (row, line)))
                .collect();
        }
        self.rendered_state
            .row_cache
            .as_ref()
            .map_or_else(Vec::new, |rows| {
                let model = self.live.rendered_model();
                let highlighter = self.live.highlighter();
                let width = self.rendered_state.last_width;
                let mut first = active.anchor.point.row.min(active.active.point.row);
                let mut last = active.anchor.point.row.max(active.active.point.row);
                while first > 0 {
                    let previous =
                        rows.rows_current_range(first - 1..first, model, highlighter, width);
                    if !previous.first().is_some_and(|(_, line)| {
                        nav::line_row_intersects_source(line, &selected, text)
                    }) {
                        break;
                    }
                    first -= 1;
                }
                while last + 1 < rows.row_count() {
                    let next =
                        rows.rows_current_range(last + 1..last + 2, model, highlighter, width);
                    if !next.first().is_some_and(|(_, line)| {
                        nav::line_row_intersects_source(line, &selected, text)
                    }) {
                        break;
                    }
                    last += 1;
                }
                rows.rows_current_range(first..last + 1, model, highlighter, width)
            })
    }

    fn selection_endpoint_at(&self, point: RenderedPoint) -> SelectionEndpoint {
        let (atom, line) = if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
            (
                nav::source_for_point(point, layout),
                nav::line_identity_for_point(point, layout),
            )
        } else {
            let line = self
                .selection_lines(point, point)
                .pop()
                .map(|(_, line)| line);
            let atom = line.as_ref().and_then(|line| {
                line.atoms
                    .iter()
                    .find(|atom| {
                        atom.columns.contains(&point.column) || atom.columns.start == point.column
                    })
                    .and_then(|atom| atom.source.clone())
            });
            let identity = atom
                .is_none()
                .then(|| {
                    self.rendered_state.row_cache.as_ref().and_then(|rows| {
                        rows.line_identity_at(
                            point.row,
                            self.live.rendered_model(),
                            self.live.highlighter(),
                            self.rendered_state.last_width,
                        )
                    })
                })
                .flatten();
            (atom, identity)
        };
        SelectionEndpoint {
            point,
            source: self.live.cursor(),
            atom,
            line,
        }
    }

    /// Check if the buffer is dirty (modified since last save).
    pub fn is_dirty(&self) -> bool {
        match self.save_point {
            SaveBaseline::Confirmed(mark) => self.live.is_modified_since(mark),
            SaveBaseline::CommittedUncertain(_) => true,
        }
    }

    /// Take a save point (marks current state as clean).
    pub fn save_point(&mut self) {
        self.save_point = SaveBaseline::Confirmed(self.live.save_point());
    }

    /// Insert pasted text in the active input mode.
    ///
    /// For bracketed paste, Insert text is inserted as one
    /// undo step with no per-character processing. In Command mode, printable
    /// ASCII path text can be appended to `:tabnew` after validating the whole
    /// paste, so terminal control characters cannot form another command.
    ///
    /// Returns `Effect::Edited` for Insert text or `Effect::Message` when a
    /// paste is rejected.
    pub fn insert_paste(&mut self, text: &str) -> Vec<Effect> {
        if let SessionMode::Command(prompt) = &mut self.session_mode {
            let path = text.trim_matches(|character| matches!(character, ' ' | '\r' | '\n'));
            let command = prompt.text.trim_end();
            if command != "tabnew" && !command.starts_with("tabnew ") {
                return vec![Effect::Message {
                    text: "paste a path after :tabnew".to_string(),
                    severity: Severity::Warning,
                }];
            }
            if path.is_empty()
                || !path.is_ascii()
                || path.chars().any(|character| character.is_ascii_control())
            {
                return vec![Effect::Message {
                    text: "pasted path must be printable ASCII on one line".to_string(),
                    severity: Severity::Warning,
                }];
            }
            if command == "tabnew" && !prompt.text.ends_with(' ') {
                prompt.text.push(' ');
            }
            prompt.text.push_str(path);
            prompt.detach();
            return Vec::new();
        }
        if self.mode() != Mode::Insert {
            return vec![Effect::Message {
                text: "paste only works in insert mode".to_string(),
                severity: Severity::Info,
            }];
        }

        let outcome = self.live.insert_text(text);
        self.translate_vim_effects(outcome)
    }

    /// Return the number of lines in the document.
    pub fn line_count(&self) -> usize {
        self.live.line_count()
    }

    /// Return a specific line (0-based), or `None` if out of range.
    pub fn line(&self, idx: usize) -> Option<String> {
        self.live.line(idx)
    }

    /// Return the display-cell column before a source character in a line.
    /// Tabs advance to four-column stops; the document text is unchanged.
    pub fn source_display_column(&self, line: usize, source_col: usize) -> usize {
        let Some(text) = self.line(line) else {
            return 0;
        };
        text.chars().take(source_col).fold(0, |column, character| {
            column + source_character_width(character, column)
        })
    }

    /// Return the source character boundary at or before a display column.
    pub fn source_column_at_display(&self, line: usize, display_col: usize) -> usize {
        let Some(text) = self.line(line) else {
            return 0;
        };
        let mut column = 0;
        let mut source_col = 0;
        for character in text.chars() {
            let next = column + source_character_width(character, column);
            if next > display_col {
                break;
            }
            column = next;
            source_col += 1;
        }
        source_col
    }

    /// Return the cursor's visual row within a document line and that line's
    /// total wrapped height at `width`.
    ///
    /// When wrapping is disabled, the result is always `(0, 1)`.
    /// For the active cursor at an exact full-width end-of-line in Insert
    /// mode, the result includes the synthetic blank continuation row used to
    /// display the insertion point.
    pub fn visual_row_info(
        &self,
        doc_line: usize,
        doc_col: usize,
        width: u16,
        wrap: bool,
    ) -> (usize, usize) {
        if !wrap {
            return (0, 1);
        }

        let line = self.line(doc_line).unwrap_or_default();
        let styled = crate::style::StyledLine {
            text: line,
            spans: Vec::new(),
        };
        let display = SourceDisplayLine::new(&styled);
        let mut wrapped = crate::rendered::wrap_source_line(&display.styled, width);
        if (doc_line, doc_col) == self.cursor()
            && self.mode() == Mode::Insert
            && Self::cursor_needs_blank_continuation(
                &display.styled.text,
                &wrapped,
                display.display_char(doc_col),
                width,
            )
        {
            wrapped.push(crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            });
        }
        let (row, _) = Self::wrapped_cursor_position(
            &display.styled.text,
            &wrapped,
            display.display_char(doc_col),
        );
        (row, wrapped.len().max(1))
    }

    /// Render the source editor frame for the given viewport.
    ///
    /// Produces a [`crate::style::SourceFrame`] containing:
    /// - Highlighted styled lines (exactly `viewport.height` lines, padded)
    /// - Cursor position in viewport-relative `(row, col)` coordinates
    /// - Search-match ranges (if any)
    ///
    /// The `Viewport.top_line` is owned by the host; the core does not
    /// modify it. The host keeps the cursor visible by adjusting
    /// `top_line` based on [`Self::cursor`] output.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::{EditorSession, Viewport};
    ///
    /// let mut session = EditorSession::from_text("# Hello\n\nWorld\n");
    /// let vp = Viewport {
    ///     top_line: 0,
    ///     height: 10,
    ///     width: 80,
    ///     wrap: true,
    ///     left_col: 0,
    ///     skip_rows: 0,
    /// };
    /// let frame = session.render_source(vp);
    /// assert_eq!(frame.lines.len(), 10); // padded to viewport height
    /// assert!(!frame.lines[0].text.is_empty()); // first line has content
    /// ```
    pub fn render_source(&mut self, vp: Viewport) -> crate::style::SourceFrame {
        self.render_source_with_atoms(vp).0
    }

    fn render_source_with_atoms(
        &mut self,
        vp: Viewport,
    ) -> (
        crate::style::SourceFrame,
        Vec<Vec<RenderedSourceAtom>>,
        Vec<usize>,
    ) {
        self.live.set_viewport(vp.top_line, vp.height);
        let line_count = self.line_count();
        let (cursor_line, cursor_col) = self.cursor();

        // Compute which document lines are visible
        let first_visible = vp.top_line;
        let last_visible = first_visible.saturating_add(vp.height as usize);

        // Highlight the visible lines (pad to viewport height)
        let start_line = first_visible.min(line_count);
        let end_line = last_visible.min(line_count);
        let mut highlighted = self
            .live
            .highlighter()
            .highlight_lines(start_line..end_line);
        highlighted.resize_with(end_line.saturating_sub(start_line), || {
            crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            }
        });
        let rendered_search = self.rendered_state.search.current().cloned();
        for (offset, styled_line) in highlighted.iter_mut().enumerate() {
            for search_match in self.live.search_matches_for_line(start_line + offset) {
                Self::overlay_search_match(styled_line, search_match);
            }
            if let Some(search) = &rendered_search {
                for start in search.find_matches(&styled_line.text) {
                    Self::overlay_search_match(styled_line, start..start + search.pattern.len());
                }
            }
        }

        // Build visual rows and their gutter metadata.
        let mut lines = Vec::with_capacity(vp.height as usize);
        let mut line_numbers = Vec::with_capacity(vp.height as usize);
        let mut source_rows = Vec::with_capacity(vp.height as usize);
        let mut source_row_offsets = Vec::with_capacity(vp.height as usize);
        let mut screen_cursor = (0usize, 0usize);
        let mut line_start = Self::source_line_start(self.live.text_ref(), start_line);

        if vp.wrap {
            for (offset, styled_line) in highlighted.iter().enumerate() {
                let doc_line = start_line + offset;
                let display = SourceDisplayLine::new(styled_line);
                let mut wrapped = crate::rendered::wrap_source_line(&display.styled, vp.width);
                if doc_line == cursor_line
                    && self.mode() == Mode::Insert
                    && Self::cursor_needs_blank_continuation(
                        &display.styled.text,
                        &wrapped,
                        display.display_char(cursor_col),
                        vp.width,
                    )
                {
                    wrapped.push(crate::style::StyledLine {
                        text: String::new(),
                        spans: Vec::new(),
                    });
                }
                let skip = if offset == 0 {
                    vp.skip_rows.min(wrapped.len().saturating_sub(1))
                } else {
                    0
                };
                let first_screen_row = lines.len();

                if doc_line == cursor_line {
                    let (wrapped_row, wrapped_col) = Self::wrapped_cursor_position(
                        &display.styled.text,
                        &wrapped,
                        display.display_char(cursor_col),
                    );
                    screen_cursor = (
                        first_screen_row + wrapped_row.saturating_sub(skip),
                        wrapped_col,
                    );
                }

                let mut display_char_start = 0;
                for (wrapped_row, row) in wrapped.into_iter().enumerate() {
                    let display_char_end = display_char_start + row.text.chars().count();
                    let row_start = display
                        .sources
                        .get(display_char_start)
                        .map_or(line_start + styled_line.text.len(), |range| {
                            line_start + range.start
                        });
                    let atoms = display.atoms(&row.text, display_char_start, line_start);
                    display_char_start = display_char_end;
                    if wrapped_row < skip {
                        continue;
                    }
                    if lines.len() == vp.height as usize {
                        break;
                    }
                    line_numbers.push(if wrapped_row == 0 {
                        Some(doc_line + 1)
                    } else {
                        None
                    });
                    source_rows.push(atoms);
                    source_row_offsets.push(row_start);
                    lines.push(row);
                }

                if lines.len() == vp.height as usize {
                    break;
                }
                line_start = Self::next_source_line_start(
                    self.live.text_ref(),
                    line_start,
                    styled_line.text.len(),
                );
            }
        } else {
            for (offset, styled_line) in highlighted.iter().enumerate() {
                if lines.len() == vp.height as usize {
                    break;
                }
                let doc_line = start_line + offset;
                let display = SourceDisplayLine::new(styled_line);
                let left_display_char = display.display_char(vp.left_col);
                // Expanded tabs can make a character-count window wider than
                // the viewport when other wide glyphs share the line.
                let window_width = if styled_line.text.contains('\t') {
                    Self::source_window_character_count(
                        &display.styled.text,
                        left_display_char,
                        vp.width,
                    )
                } else {
                    vp.width
                };
                let show_left_indicator = left_display_char > 0
                    && !(styled_line.text.contains('\t')
                        && doc_line == cursor_line
                        && vp.left_col == cursor_col);
                if doc_line == cursor_line {
                    screen_cursor = (
                        lines.len(),
                        display
                            .display_char(cursor_col)
                            .saturating_sub(left_display_char)
                            .min(vp.width.saturating_sub(1) as usize),
                    );
                }
                source_rows.push(Self::horizontal_window_atoms(
                    &display,
                    line_start,
                    left_display_char,
                    window_width,
                    show_left_indicator,
                ));
                source_row_offsets.push(line_start);
                lines.push(Self::horizontal_window(
                    &display.styled,
                    left_display_char,
                    window_width,
                    show_left_indicator,
                ));
                line_numbers.push(Some(doc_line + 1));
                line_start = Self::next_source_line_start(
                    self.live.text_ref(),
                    line_start,
                    styled_line.text.len(),
                );
            }
        }

        // Pad with blank lines if we have fewer lines than viewport height
        while lines.len() < vp.height as usize {
            lines.push(crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            });
            line_numbers.push(None);
            source_rows.push(Vec::new());
            source_row_offsets.push(self.live.text_ref().len());
        }

        // Truncate to exactly viewport.height (in case we over-highlighted)
        lines.truncate(vp.height as usize);
        line_numbers.truncate(vp.height as usize);
        source_rows.truncate(vp.height as usize);
        source_row_offsets.truncate(vp.height as usize);

        let decorations = self
            .diagnostics_for_source_rows(&source_rows)
            .into_iter()
            .flat_map(|diagnostic| {
                let kind = DecorationKind::Diagnostic {
                    provider: diagnostic.provider,
                    severity: diagnostic.severity,
                };
                source_rows
                    .iter()
                    .enumerate()
                    .flat_map(move |(row, atoms)| {
                        nav::project_atom_intervals(&diagnostic.range, atoms)
                            .into_iter()
                            .map(move |columns| SourceDecoration { row, columns, kind })
                    })
            })
            .collect();

        let frame = crate::style::SourceFrame {
            lines,
            decorations,
            line_numbers,
            first_line_number: first_visible + 1,
            cursor: (
                screen_cursor.0.min(vp.height.saturating_sub(1) as usize) as u16,
                screen_cursor.1.min(vp.width.saturating_sub(1) as usize) as u16,
            ),
        };
        (frame, source_rows, source_row_offsets)
    }

    /// Return the source byte offset under a source-view viewport cell.
    ///
    /// Cells within a wide character resolve to its first byte. A click past
    /// the visible text resolves to the end of that row. Window indicators
    /// resolve to the nearest source-backed cell; empty rows use their source
    /// row start, or logical EOF below the document.
    pub fn source_offset_at_viewport_cell(
        &mut self,
        viewport: Viewport,
        row: usize,
        column: usize,
    ) -> Option<usize> {
        if row >= usize::from(viewport.height) || column >= usize::from(viewport.width) {
            return None;
        }
        let (_, source_rows, row_offsets) = self.render_source_with_atoms(viewport);
        let atoms = source_rows.get(row)?;
        let nearest = atoms
            .iter()
            .filter(|atom| atom.source.is_some())
            .min_by_key(|atom| {
                if column < atom.columns.start {
                    atom.columns.start - column
                } else if column >= atom.columns.end {
                    column - atom.columns.end + 1
                } else {
                    0
                }
            });
        if let Some(atom) = nearest {
            let source = atom.source.as_ref()?;
            return Some(if column >= atom.columns.end {
                source.end
            } else {
                source.start
            });
        }
        row_offsets.get(row).copied()
    }

    fn diagnostics_for_source_rows<'a>(
        &'a self,
        source_rows: &[Vec<RenderedSourceAtom>],
    ) -> Vec<&'a Diagnostic> {
        let intervals =
            Self::visible_source_intervals(source_rows.iter().map(|atoms| atoms.as_slice()));
        let diagnostics = self.diagnostics();
        Self::visible_diagnostic_indices(diagnostics, &intervals)
            .into_iter()
            .map(|index| &diagnostics[index])
            .collect()
    }

    fn visible_source_intervals<'a>(
        rows: impl Iterator<Item = &'a [RenderedSourceAtom]>,
    ) -> Vec<Range<usize>> {
        let mut intervals = rows
            .flat_map(|atoms| atoms.iter().filter_map(|atom| atom.source.clone()))
            .filter(|source| source.start < source.end)
            .collect::<Vec<_>>();
        intervals.sort_by_key(|source| (source.start, source.end));

        let mut merged: Vec<Range<usize>> = Vec::new();
        for source in intervals {
            if let Some(previous) = merged.last_mut() {
                if source.start <= previous.end {
                    previous.end = previous.end.max(source.end);
                    continue;
                }
            }
            merged.push(source);
        }
        merged
    }

    fn visible_diagnostic_indices(
        diagnostics: &[Diagnostic],
        source_intervals: &[Range<usize>],
    ) -> Vec<usize> {
        let mut indices = Vec::new();
        let Some(first_source) = source_intervals.first() else {
            return indices;
        };
        let mut diagnostic_index =
            diagnostics.partition_point(|diagnostic| diagnostic.range.end <= first_source.start);
        for source in source_intervals {
            while diagnostic_index < diagnostics.len()
                && diagnostics[diagnostic_index].range.end <= source.start
            {
                diagnostic_index += 1;
            }
            while diagnostic_index < diagnostics.len()
                && diagnostics[diagnostic_index].range.start < source.end
            {
                indices.push(diagnostic_index);
                diagnostic_index += 1;
            }
        }
        indices
    }

    fn source_line_start(text: &str, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        text.match_indices('\n')
            .nth(line - 1)
            .map_or(text.len(), |(offset, _)| offset + 1)
    }

    fn next_source_line_start(text: &str, start: usize, line_len: usize) -> usize {
        let end = start.saturating_add(line_len).min(text.len());
        end + usize::from(text.as_bytes().get(end) == Some(&b'\n'))
    }

    fn horizontal_window_atoms(
        display: &SourceDisplayLine,
        source_start: usize,
        left_col: usize,
        width: u16,
        show_left_indicator: bool,
    ) -> Vec<RenderedSourceAtom> {
        let chars: Vec<char> = display.styled.text.chars().collect();
        let width = usize::from(width);
        if width == 0 || left_col >= chars.len() {
            return Vec::new();
        }
        let end_col = left_col.saturating_add(width).min(chars.len());
        let right_clipped = chars.len() > end_col;
        let mut atoms: Vec<RenderedSourceAtom> = Vec::new();
        let mut display_column = 0usize;
        for (window_index, &character) in chars[left_col..end_col].iter().enumerate() {
            let synthetic = (show_left_indicator && window_index == 0)
                || (right_clipped && window_index + 1 == end_col - left_col);
            let display_width = if synthetic {
                1
            } else {
                character.width().unwrap_or(0)
            };
            let range = &display.sources[left_col + window_index];
            let source =
                (!synthetic).then_some(source_start + range.start..source_start + range.end);
            if display_width == 0 {
                if let (Some(previous), Some(source)) = (atoms.last_mut(), source) {
                    if let Some(previous_source) = previous.source.as_mut() {
                        previous_source.end = source.end;
                    }
                }
                continue;
            }
            atoms.push(RenderedSourceAtom {
                columns: display_column..display_column + display_width,
                source,
            });
            display_column += display_width;
        }
        atoms
    }

    fn source_window_character_count(text: &str, left_char: usize, width: u16) -> u16 {
        if width == 0 {
            return 0;
        }
        let mut cells = 0;
        let mut characters = 0;
        for character in text.chars().skip(left_char) {
            let character_width = character.width().unwrap_or(0);
            if characters > 0 && cells + character_width > usize::from(width) {
                break;
            }
            if character_width > usize::from(width) {
                return 1;
            }
            cells += character_width;
            characters += 1;
        }
        characters.min(usize::from(u16::MAX)) as u16
    }

    fn wrapped_cursor_position(
        source: &str,
        wrapped: &[crate::style::StyledLine],
        doc_col: usize,
    ) -> (usize, usize) {
        let chars: Vec<char> = source.chars().collect();
        let doc_col = doc_col.min(chars.len());
        let mut source_pos = 0usize;

        for (row, styled) in wrapped.iter().enumerate() {
            let row_start = source_pos;
            let row_len = styled.text.chars().count();
            let row_end = (row_start + row_len).min(chars.len());

            if doc_col < row_end {
                return (row, doc_col.saturating_sub(row_start));
            }
            if doc_col == row_end {
                if row + 1 < wrapped.len() {
                    return (row + 1, 0);
                }
                return (row, row_len);
            }

            source_pos = row_end;
        }

        let last = wrapped.len().saturating_sub(1);
        (
            last,
            wrapped
                .get(last)
                .map_or(0, |line| line.text.chars().count()),
        )
    }

    fn cursor_needs_blank_continuation(
        source: &str,
        wrapped: &[crate::style::StyledLine],
        doc_col: usize,
        width: u16,
    ) -> bool {
        width > 0
            && doc_col == source.chars().count()
            && wrapped.last().is_some_and(|line| {
                unicode_width::UnicodeWidthStr::width(line.text.as_str()) >= width as usize
            })
    }

    fn horizontal_window(
        styled_line: &crate::style::StyledLine,
        left_col: usize,
        width: u16,
        show_left_indicator: bool,
    ) -> crate::style::StyledLine {
        let chars: Vec<char> = styled_line.text.chars().collect();
        let width = width as usize;
        if width == 0 || left_col >= chars.len() {
            return crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            };
        }

        let end_col = left_col.saturating_add(width).min(chars.len());
        let text: String = chars[left_col..end_col].iter().collect();
        let spans = styled_line
            .spans
            .iter()
            .filter_map(|span| {
                let start = span.start_col.max(left_col);
                let end = span.end_col.min(end_col);
                (start < end).then_some(crate::style::Span {
                    start_col: start - left_col,
                    end_col: end - left_col,
                    style: span.style,
                })
            })
            .collect();
        let mut window = crate::style::StyledLine { text, spans };

        if show_left_indicator {
            Self::replace_window_character(&mut window, 0, '«', crate::style::SemanticStyle::Muted);
        }
        if chars.len() > left_col.saturating_add(width) {
            Self::replace_window_character(
                &mut window,
                width.saturating_sub(1),
                '»',
                crate::style::SemanticStyle::Muted,
            );
        }

        window
    }

    fn replace_window_character(
        line: &mut crate::style::StyledLine,
        col: usize,
        replacement: char,
        style: crate::style::SemanticStyle,
    ) {
        let mut chars: Vec<char> = line.text.chars().collect();
        if col >= chars.len() {
            return;
        }
        chars[col] = replacement;
        line.text = chars.into_iter().collect();

        let mut spans = Vec::with_capacity(line.spans.len() + 1);
        for span in &line.spans {
            if span.end_col <= col || span.start_col > col {
                spans.push(span.clone());
                continue;
            }
            if span.start_col < col {
                spans.push(crate::style::Span {
                    start_col: span.start_col,
                    end_col: col,
                    style: span.style,
                });
            }
            if span.end_col > col + 1 {
                spans.push(crate::style::Span {
                    start_col: col + 1,
                    end_col: span.end_col,
                    style: span.style,
                });
            }
        }
        spans.push(crate::style::Span {
            start_col: col,
            end_col: col + 1,
            style,
        });
        spans.sort_by_key(|span| span.start_col);
        line.spans = spans;
    }

    /// Render the Normal/Select Markdown layout at the host's text width.
    ///
    /// Builds (or returns a reference to the cached layout for) a
    /// [`crate::style::RenderedLayout`] from the current document text,
    /// highlighter, and block model.
    ///
    /// The layout is invalidated on edits and width changes. Callers should
    /// pass the current terminal width on each call.
    ///
    /// # Example
    ///
    /// ```
    /// use oom_edit_core::EditorSession;
    ///
    /// let mut session = EditorSession::from_text("# Hello\n\n* item\n");
    /// let layout = session.render_layout(80);
    /// assert!(!layout.lines.is_empty());
    /// ```
    pub fn render_layout(&mut self, width: u16) -> &crate::style::RenderedLayout {
        if self.rendered_state.needs_layout(width) {
            let source_anchor = self.live.cursor();
            let selection = match &self.session_mode {
                SessionMode::Select(selection) => Some(selection.clone()),
                SessionMode::CoreDriven | SessionMode::Command(_) => None,
            };
            let character_selection = selection
                .as_ref()
                .is_some_and(|selection| matches!(selection.kind, SelectionKind::Character { .. }));
            let block_selection = selection
                .as_ref()
                .is_some_and(|selection| matches!(selection.kind, SelectionKind::Block));
            let active_atom_remap = character_selection
                || (block_selection
                    && selection
                        .as_ref()
                        .is_some_and(|selection| selection.active.atom.is_some()));
            let anchor_atom_remap = character_selection
                || (block_selection
                    && selection
                        .as_ref()
                        .is_some_and(|selection| selection.anchor.atom.is_some()));
            let active_line_remap = selection.as_ref().is_some_and(|selection| {
                selection.active.atom.is_none() && selection.active.line.is_some()
            });
            let anchor_line_remap = selection.as_ref().is_some_and(|selection| {
                selection.anchor.atom.is_none() && selection.anchor.line.is_some()
            });
            let text = self.live.text();
            self.live.ensure_rendered_model();
            let model = self.live.rendered_model();
            let (layout, code_fence_regions, boundaries) = if let Some(rows) =
                self.rendered_state.row_cache.take()
            {
                if self.rendered_state.last_width == width {
                    let (layout, fences, boundaries) =
                        rows.into_complete_current(model, &text, self.live.highlighter(), width);
                    (layout, fences, boundaries)
                } else {
                    RenderedLayout::build_with_boundaries(
                        model,
                        width,
                        self.live.highlighter(),
                        self.rendered_state.fm_collapsed,
                    )
                }
            } else {
                RenderedLayout::build_with_boundaries(
                    model,
                    width,
                    self.live.highlighter(),
                    self.rendered_state.fm_collapsed,
                )
            };
            #[cfg(test)]
            let last_work = ProjectionWork {
                rebuilt_blocks: self.live.rendered_model_work().rebuilt_blocks,
                reused_blocks: self.live.rendered_model_work().reused_blocks,
                rebuilt_rows: layout.lines.len(),
                reused_rows: 0,
            };
            let cursor = active_atom_remap
                .then(|| {
                    selection
                        .as_ref()
                        .and_then(|selection| selection.active.atom.as_ref())
                        .and_then(|source| nav::point_for_source_range(source, &layout))
                })
                .flatten()
                .or_else(|| {
                    active_line_remap
                        .then(|| {
                            selection
                                .as_ref()
                                .and_then(|selection| selection.active.line.as_ref())
                                .and_then(|(source, ordinal)| {
                                    nav::point_for_line_identity(
                                        source,
                                        *ordinal,
                                        self.rendered_state.cursor.column,
                                        &layout,
                                    )
                                })
                        })
                        .flatten()
                })
                .map(RenderedCursor::at)
                .unwrap_or_else(|| {
                    nav::enter_rendered_indexed(
                        source_anchor.0,
                        source_anchor.1,
                        &layout,
                        &text,
                        self.live.highlighter().line_starts(),
                    )
                });
            let select_anchor = anchor_atom_remap
                .then(|| {
                    selection
                        .as_ref()
                        .and_then(|selection| selection.anchor.atom.as_ref())
                        .and_then(|source| nav::point_for_source_range(source, &layout))
                })
                .flatten()
                .or_else(|| {
                    anchor_line_remap
                        .then(|| {
                            selection
                                .as_ref()
                                .and_then(|selection| selection.anchor.line.as_ref())
                                .and_then(|(source, ordinal)| {
                                    nav::point_for_line_identity(
                                        source,
                                        *ordinal,
                                        selection
                                            .as_ref()
                                            .map_or(0, |selection| selection.anchor.point.column),
                                        &layout,
                                    )
                                })
                        })
                        .flatten()
                })
                .or_else(|| {
                    selection.as_ref().map(|selection| {
                        let (line, col) = selection.anchor.source;
                        nav::enter_rendered_indexed(
                            line,
                            col,
                            &layout,
                            &text,
                            self.live.highlighter().line_starts(),
                        )
                        .point()
                    })
                });
            self.rendered_state.layout_cache = Some(layout);
            self.rendered_state.layout_boundaries = boundaries;
            self.rendered_state.code_fence_regions = code_fence_regions;
            self.rendered_state.last_width = width;
            self.rendered_state.cursor = cursor;
            if let SessionMode::Select(selection) = &mut self.session_mode {
                if let Some(select_anchor) = select_anchor {
                    selection.anchor.point = select_anchor;
                }
                selection.active.point = cursor.point();
            }
            #[cfg(test)]
            {
                self.rendered_state.layout_builds += 1;
                self.rendered_state.last_work = last_work;
            }
        }
        self.rendered_state
            .layout_cache
            .as_ref()
            .expect("rendered layout must be cached after building")
    }

    fn ensure_rendered_rows(&mut self, width: u16) {
        if width == 0
            || (self.rendered_state.row_cache.is_some() && self.rendered_state.last_width == width)
        {
            return;
        }
        self.render_layout(width);
        let layout = self
            .rendered_state
            .layout_cache
            .take()
            .expect("flat layout was built before partitioning");
        let fences = std::mem::take(&mut self.rendered_state.code_fence_regions);
        let boundaries = std::mem::take(&mut self.rendered_state.layout_boundaries);
        let rows = if boundaries.is_empty() {
            self.live.ensure_rendered_model();
            RetainedRows::build(
                self.live.rendered_model(),
                width,
                self.live.highlighter(),
                self.rendered_state.fm_collapsed,
            )
        } else {
            RetainedRows::from_complete(layout, fences, &boundaries)
        };
        self.rendered_state.code_fence_regions = rows.fence_regions();
        self.rendered_state.row_cache = Some(rows);
    }

    /// Return only the current rendered rows needed by a host viewport.
    ///
    /// Unlike [`Self::render_layout`], a local edit can retain unaffected
    /// mapped rows and read only the requested window. The cursor and row
    /// count still use document-wide coordinates.
    pub fn rendered_viewport(
        &mut self,
        width: u16,
        top: usize,
        height: usize,
    ) -> crate::style::RenderedViewportFrame {
        if self.rendered_state.row_cache.is_none() || self.rendered_state.last_width != width {
            self.render_layout(width);
        }
        if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
            let total = layout.lines.len();
            let first = top.min(total.saturating_sub(1));
            let end = first.saturating_add(height).min(total);
            let lines = layout.lines[first..end].to_vec();
            let numbers = layout.line_numbers[first..end].to_vec();
            let mut source_line = layout.line_numbers[..first]
                .iter()
                .rev()
                .find_map(|number| *number)
                .map(|number| number - 1);
            let mut previous = first.checked_sub(1).and_then(|row| layout.lines.get(row));
            let mut continuations = Vec::with_capacity(lines.len());
            for (line, number) in lines.iter().zip(&numbers) {
                let continuation = if let Some(number) = number {
                    source_line = Some(number - 1);
                    None
                } else if previous.is_some_and(|prior| {
                    line.kind == LineKind::Content && line.source == prior.source
                }) {
                    source_line
                } else {
                    None
                };
                continuations.push(continuation);
                previous = Some(line);
            }
            return crate::style::RenderedViewportFrame {
                first_row: first,
                total_rows: total,
                lines,
                line_numbers: numbers,
                gutter_continuations: continuations,
                cursor: self.rendered_cursor(),
            };
        }
        self.ensure_rendered_rows(width);
        let Some(rows) = self.rendered_state.row_cache.as_mut() else {
            return crate::style::RenderedViewportFrame {
                first_row: 0,
                total_rows: 0,
                lines: Vec::new(),
                line_numbers: Vec::new(),
                gutter_continuations: Vec::new(),
                cursor: self.rendered_cursor(),
            };
        };
        let total = rows.row_count();
        let first = top.min(total.saturating_sub(1));
        let end = first.saturating_add(height).min(total);
        let mut prior_source_line = None;
        let mut prior_line = None;
        if first > 0 {
            prior_line = rows.row_current(
                first - 1,
                self.live.rendered_model(),
                self.live.text_ref(),
                self.live.highlighter(),
                width,
            );
            for previous in (0..first).rev() {
                if let Some(number) = rows.line_number(previous) {
                    prior_source_line = Some(number - 1);
                    break;
                }
            }
        }
        let mut lines = Vec::with_capacity(end - first);
        let mut numbers = Vec::with_capacity(end - first);
        let mut continuations = Vec::with_capacity(end - first);
        for row in first..end {
            let line = rows
                .row_current(
                    row,
                    self.live.rendered_model(),
                    self.live.text_ref(),
                    self.live.highlighter(),
                    width,
                )
                .expect("indexed viewport row is within the current geometry");
            let number = rows.line_number(row);
            let continuation = if let Some(number) = number {
                prior_source_line = Some(number - 1);
                None
            } else if prior_line
                .as_ref()
                .is_some_and(|previous: &crate::style::RenderedLine| {
                    line.kind == LineKind::Content && line.source == previous.source
                })
            {
                prior_source_line
            } else {
                None
            };
            prior_line = Some(line.clone());
            lines.push(line);
            numbers.push(number);
            continuations.push(continuation);
        }
        crate::style::RenderedViewportFrame {
            first_row: first,
            total_rows: total,
            lines,
            line_numbers: numbers,
            gutter_continuations: continuations,
            cursor: self.rendered_cursor(),
        }
    }

    /// Return the rendered cursor row.
    ///
    /// Used by the host to implement scrolling: the host keeps the cursor
    /// visible by adjusting `Viewport.top_line` based on this value.
    pub fn rendered_cursor_line(&self) -> usize {
        self.rendered_state.cursor.line
    }

    /// Remap the rendered cursor from canonical source coordinates.
    ///
    /// When the terminal width changes, the layout re-wraps and rendered row
    /// indices shift. This remaps the rendered cursor to the same content
    /// line using the core's `enter_rendered` pure function.
    pub fn remap_rendered_cursor(&mut self, edit_line: usize, edit_col: usize) {
        if self.rendered_state.layout_cache.is_none() && self.rendered_state.row_cache.is_some() {
            self.render_layout(self.rendered_state.last_width);
        }
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        self.rendered_state.cursor = nav::enter_rendered_indexed(
            edit_line,
            edit_col,
            layout,
            self.live.text_ref(),
            self.live.highlighter().line_starts(),
        );
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Handle keys in Command mode (ex-command entry).
    fn handle_command_mode_key(&mut self, key: KeyInput) -> Vec<Effect> {
        let mut effects = Vec::new();
        let SessionMode::Command(prompt) = &mut self.session_mode else {
            return effects;
        };
        match key.code.kind {
            KeyCodeKind::Esc => {
                // Cancel command-line and return to Normal
                self.session_mode = SessionMode::CoreDriven;
                effects.push(Effect::ModeChanged(Mode::Normal));
            }
            KeyCodeKind::Enter => {
                // Execute the command from the buffer
                let cmd = prompt.text.trim().to_string();
                if !cmd.is_empty() {
                    self.command_history.record(cmd.clone());
                }
                effects.extend(self.process_ex_command(&cmd));
                // Only default to Normal if the ex command didn't already change mode
                if !effects.iter().any(|e| matches!(e, Effect::ModeChanged(_))) {
                    self.session_mode = SessionMode::CoreDriven;
                    effects.push(Effect::ModeChanged(Mode::Normal));
                }
            }
            KeyCodeKind::Backspace => {
                // Remove last character from command buffer
                prompt.text.pop();
                prompt.detach();
            }
            KeyCodeKind::Up if key.mods == Modifiers::default() => {
                prompt.up(&self.command_history);
            }
            KeyCodeKind::Down if key.mods == Modifiers::default() => {
                prompt.down(&self.command_history);
            }
            _ => {
                // Collect printable characters in the command buffer
                if let KeyCodeKind::Char(c) = key.code.kind {
                    if !key.mods.ctrl && !key.mods.alt && !key.mods.shift {
                        prompt.text.push(c);
                        prompt.detach();
                    }
                }
            }
        }
        effects
    }

    fn handle_insert_key(&mut self, key: KeyInput) -> Vec<Effect> {
        let vim_effects = self.live.handle_key(key);
        self.translate_vim_effects(vim_effects)
    }

    fn handle_rendered_normal_key(&mut self, key: KeyInput) -> Vec<Effect> {
        if self.rendered_state.search.prompt().is_some() {
            return self.handle_rendered_search_input(key);
        }
        if self.rendered_state.register_input == RegisterInput::AwaitingName {
            if let KeyCodeKind::Char(selector) = key.code.kind {
                if key.mods == Modifiers::default() {
                    self.rendered_state.register_input.select(selector);
                    return Vec::new();
                }
            }
            self.rendered_state.register_input = RegisterInput::Default;
        }
        if self.live.has_pending_input() {
            return self.forward_key_to_vim(key);
        }
        if let Some(effects) = self.rendered_tab_effect(key) {
            return effects;
        }
        if let Some(effects) = self.resolve_pending_spell_bracket(key) {
            return effects;
        }
        if let Some(effects) = self.forward_pending_native_g(key) {
            return effects;
        }
        if self.first_rendered_g_is_pending(key) {
            return Vec::new();
        }
        if self.first_heading_bracket_is_pending(key) {
            return Vec::new();
        }
        if key.mods == Modifiers::default()
            && matches!(key.code.kind, KeyCodeKind::Char('y') | KeyCodeKind::Enter)
        {
            if let Some(destination) = self.focused_synthetic_link_destination() {
                self.rendered_state.count = 0;
                self.rendered_state.register_input = RegisterInput::Default;
                return vec![Effect::ClipboardWrite(
                    crate::clipboard::ClipboardContent::invariant(destination),
                )];
            }
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('v' | 'V')) {
            return self.enter_select(SelectionShape::Block);
        }
        if !key.mods.ctrl && !key.mods.alt && key.code.kind == KeyCodeKind::Char('V') {
            return self.enter_select(SelectionShape::Line);
        }
        if key.mods == Modifiers::default() {
            match key.code.kind {
                KeyCodeKind::Char('v') => {
                    return self.enter_select(SelectionShape::Character);
                }
                KeyCodeKind::Char('"') => {
                    self.rendered_state.register_input = RegisterInput::AwaitingName;
                    return Vec::new();
                }
                KeyCodeKind::Char(':') => {
                    self.rendered_state.search.cancel();
                    self.session_mode = SessionMode::Command(CommandPrompt::default());
                    return vec![Effect::ModeChanged(Mode::Command)];
                }
                KeyCodeKind::Char('i') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::Insert)
                }
                KeyCodeKind::Char('a') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::Append)
                }
                KeyCodeKind::Char('I') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::InsertLineStart)
                }
                KeyCodeKind::Char('A') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::AppendLineEnd)
                }
                KeyCodeKind::Char('o') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::OpenBelow)
                }
                KeyCodeKind::Char('O') => {
                    return self.enter_insert_from_rendered(RenderedExitAction::OpenAbove)
                }
                KeyCodeKind::Char('p') | KeyCodeKind::Char('P') | KeyCodeKind::Char('u') => {
                    let mut vim_effects = Vec::new();
                    if matches!(key.code.kind, KeyCodeKind::Char('p' | 'P')) {
                        if let Some(selector) = self.rendered_state.register_input.take().selector()
                        {
                            vim_effects.extend(self.live.handle_key(KeyInput {
                                code: KeyCode {
                                    kind: KeyCodeKind::Char('"'),
                                },
                                mods: Modifiers::default(),
                            }));
                            vim_effects.extend(self.live.handle_key(KeyInput {
                                code: KeyCode {
                                    kind: KeyCodeKind::Char(selector),
                                },
                                mods: Modifiers::default(),
                            }));
                        }
                    }
                    vim_effects.extend(self.live.handle_key(key));
                    let mut effects = self.translate_vim_effects(vim_effects);
                    self.session_mode = SessionMode::CoreDriven;
                    effects.retain(|effect| !matches!(effect, Effect::ModeChanged(_)));
                    return effects;
                }
                _ => {}
            }
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('r')) {
            let vim_effects = self.live.handle_key(key);
            let mut effects = self.translate_vim_effects(vim_effects);
            self.session_mode = SessionMode::CoreDriven;
            effects.retain(|effect| !matches!(effect, Effect::ModeChanged(_)));
            return effects;
        }
        self.handle_rendered_navigation_key(key)
    }

    fn handle_rendered_select_key(&mut self, key: KeyInput) -> Vec<Effect> {
        if self.rendered_state.search.prompt().is_some() {
            return self.handle_rendered_search_input(key);
        }
        if self.rendered_state.register_input == RegisterInput::AwaitingName {
            if let KeyCodeKind::Char(selector) = key.code.kind {
                if key.mods == Modifiers::default() {
                    self.rendered_state.register_input.select(selector);
                    return Vec::new();
                }
            }
            self.rendered_state.register_input = RegisterInput::Default;
        }
        if let Some(effects) = self.rendered_tab_effect(key) {
            return effects;
        }
        if self.first_rendered_g_is_pending(key) {
            return Vec::new();
        }
        if self.first_heading_bracket_is_pending(key) {
            return Vec::new();
        }
        let plain_text_yank =
            matches!(key.code.kind, KeyCodeKind::Char('Y')) && !key.mods.ctrl && !key.mods.alt;
        let default_yank =
            key.mods == Modifiers::default() && matches!(key.code.kind, KeyCodeKind::Char('y'));
        let enter = key.mods == Modifiers::default() && key.code.kind == KeyCodeKind::Enter;
        if default_yank || plain_text_yank || enter {
            let copy_focused_link = matches!(key.code.kind, KeyCodeKind::Enter)
                || self
                    .rendered_selection()
                    .is_some_and(|selection| selection.source_ranges.is_empty());
            if let (true, Some(destination)) =
                (copy_focused_link, self.focused_synthetic_link_destination())
            {
                self.rendered_state.count = 0;
                if enter {
                    self.rendered_state.register_input = RegisterInput::Default;
                    return vec![Effect::ClipboardWrite(
                        crate::clipboard::ClipboardContent::invariant(destination),
                    )];
                }
                let register = self.rendered_state.register_input.take();
                let effects = if matches!(register, Register::Unnamed | Register::System) {
                    vec![Effect::ClipboardWrite(
                        crate::clipboard::ClipboardContent::invariant(destination),
                    )]
                } else {
                    Vec::new()
                };
                return self.finish_select(Mode::Normal, effects);
            }
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('c')) {
            return self.finish_select(Mode::Normal, Vec::new());
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('v' | 'V')) {
            return self.switch_or_cancel_selection_shape(SelectionShape::Block);
        }
        if !key.mods.ctrl && !key.mods.alt && key.code.kind == KeyCodeKind::Char('V') {
            return self.switch_or_cancel_selection_shape(SelectionShape::Line);
        }
        if plain_text_yank {
            return self.apply_select_operator(RangeOperator::Yank, YankPublication::PlainText);
        }
        if key.mods == Modifiers::default() {
            match key.code.kind {
                KeyCodeKind::Esc => return self.finish_select(Mode::Normal, Vec::new()),
                KeyCodeKind::Char('v') => {
                    return self.switch_or_cancel_selection_shape(SelectionShape::Character)
                }
                KeyCodeKind::Char('o') => {
                    let SessionMode::Select(selection) = &mut self.session_mode else {
                        return Vec::new();
                    };
                    selection.swap_endpoints();
                    let active = selection.active.clone();
                    self.rendered_state.cursor = RenderedCursor::at(active.point);
                    self.live.jump_to(active.source.0, active.source.1);
                    return vec![Effect::CursorMoved];
                }
                KeyCodeKind::Char('"') => {
                    self.rendered_state.register_input = RegisterInput::AwaitingName;
                    return Vec::new();
                }
                KeyCodeKind::Char('y') => {
                    return self
                        .apply_select_operator(RangeOperator::Yank, YankPublication::Configured)
                }
                KeyCodeKind::Char('d') | KeyCodeKind::Char('x') => {
                    return self
                        .apply_select_operator(RangeOperator::Delete, YankPublication::Configured)
                }
                KeyCodeKind::Char('c') => {
                    return self
                        .apply_select_operator(RangeOperator::Change, YankPublication::Configured)
                }
                KeyCodeKind::Char('>') => {
                    return self
                        .apply_select_operator(RangeOperator::Indent, YankPublication::Configured)
                }
                KeyCodeKind::Char('<') => {
                    return self
                        .apply_select_operator(RangeOperator::Outdent, YankPublication::Configured)
                }
                _ => {
                    self.rendered_state.register_input = RegisterInput::Default;
                }
            }
        }
        self.handle_rendered_navigation_key(key)
    }

    fn focused_synthetic_link_destination(&self) -> Option<String> {
        let line = self.rendered_state.cursor.line;
        if let Some(rows) = self.rendered_state.row_cache.as_ref() {
            return rows.link_destination_at_row(line).map(ToOwned::to_owned);
        }
        let layout = self.rendered_state.layout_cache.as_ref()?;
        if layout.lines.get(line)?.kind != LineKind::Synthetic {
            return None;
        }
        let index = layout.jump_targets.iter().find_map(|target| {
            if target.line != line {
                return None;
            }
            match target.kind {
                TargetKind::Link(index) => Some(index),
                TargetKind::Heading(_) | TargetKind::Footnote => None,
            }
        })?;
        layout
            .link_index
            .get(index)
            .map(|(_, destination)| destination.clone())
    }

    fn rendered_register(selector: char) -> Option<Register> {
        match selector {
            '+' | '*' => Some(Register::System),
            '_' => Some(Register::BlackHole),
            name @ ('a'..='z' | 'A'..='Z' | '0'..='9' | '-') => Some(Register::Named(name)),
            _ => None,
        }
    }

    fn rendered_tab_effect(&mut self, key: KeyInput) -> Option<Vec<Effect>> {
        if !self.rendered_state.pending_g || key.mods != Modifiers::default() {
            return None;
        }
        let effect = match key.code.kind {
            KeyCodeKind::Char('t') => {
                let count = std::mem::take(&mut self.rendered_state.count);
                if count == 0 {
                    Effect::TabNext
                } else {
                    Effect::TabJump {
                        one_based: std::num::NonZeroUsize::new(count)
                            .expect("positive count is required for counted gt"),
                    }
                }
            }
            KeyCodeKind::Char('T') => {
                self.rendered_state.count = 0;
                Effect::TabPrev
            }
            _ => return None,
        };
        self.rendered_state.pending_g = false;
        Some(vec![effect])
    }

    /// Replay a rendered `g` prefix into Vim when it is not one of the
    /// renderer-owned `gg`/`gt`/`gT` commands. Counts are replayed too, so the
    /// native parser receives the exact sequence the host supplied.
    fn forward_pending_native_g(&mut self, key: KeyInput) -> Option<Vec<Effect>> {
        if !self.rendered_state.pending_g || key.mods != Modifiers::default() {
            return None;
        }
        if matches!(key.code.kind, KeyCodeKind::Char('g' | 't' | 'T')) {
            return None;
        }

        self.rendered_state.pending_g = false;
        self.commit_rendered_cursor();
        let count = std::mem::take(&mut self.rendered_state.count);
        let mut vim_effects = Vec::new();
        if count > 0 {
            for digit in count.to_string().chars() {
                vim_effects.extend(self.live.handle_key(KeyInput {
                    code: KeyCode {
                        kind: KeyCodeKind::Char(digit),
                    },
                    mods: Modifiers::default(),
                }));
            }
        }
        vim_effects.extend(self.live.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char('g'),
            },
            mods: Modifiers::default(),
        }));
        vim_effects.extend(self.live.handle_key(key));
        Some(self.translate_vim_effects(vim_effects))
    }

    fn forward_key_to_vim(&mut self, key: KeyInput) -> Vec<Effect> {
        let vim_effects = self.live.handle_key(key);
        self.translate_vim_effects(vim_effects)
    }

    /// Hold the first `g` so only the complete rendered `gg` motion jumps.
    fn first_rendered_g_is_pending(&mut self, key: KeyInput) -> bool {
        let is_plain_g =
            key.mods == Modifiers::default() && matches!(key.code.kind, KeyCodeKind::Char('g'));
        if !is_plain_g {
            self.rendered_state.pending_g = false;
            return false;
        }
        if self.rendered_state.pending_g {
            self.rendered_state.pending_g = false;
            false
        } else {
            self.rendered_state.pending_g = true;
            true
        }
    }

    /// Hold the first bracket of `[[`/`]]`; the renderer uses count two to
    /// distinguish the completed heading motion from an unbound bracket.
    fn first_heading_bracket_is_pending(&mut self, key: KeyInput) -> bool {
        let bracket = match key.code.kind {
            KeyCodeKind::Char(c @ ('[' | ']')) if key.mods == Modifiers::default() => c,
            _ => {
                self.rendered_state.pending_heading_bracket = None;
                return false;
            }
        };
        if self.rendered_state.pending_heading_bracket == Some(bracket) {
            self.rendered_state.pending_heading_bracket = None;
            self.rendered_state.count = self.rendered_state.count.saturating_add(1).max(2);
            false
        } else {
            self.rendered_state.pending_heading_bracket = Some(bracket);
            true
        }
    }

    fn resolve_pending_spell_bracket(&mut self, key: KeyInput) -> Option<Vec<Effect>> {
        let bracket = self.rendered_state.pending_heading_bracket?;
        if key.mods == Modifiers::default() && matches!(key.code.kind, KeyCodeKind::Char('s')) {
            self.rendered_state.pending_heading_bracket = None;
            let count = std::mem::take(&mut self.rendered_state.count).max(1);
            return Some(self.navigate_diagnostic(bracket == ']', count));
        }
        if key.mods == Modifiers::default()
            && matches!(key.code.kind, KeyCodeKind::Char(current) if current == bracket)
        {
            return None;
        }

        self.rendered_state.pending_heading_bracket = None;
        self.commit_rendered_cursor();
        let mut effects = self.live.handle_key(KeyInput {
            code: KeyCode {
                kind: KeyCodeKind::Char(bracket),
            },
            mods: Modifiers::default(),
        });
        effects.effects.extend(self.live.handle_key(key).effects);
        Some(self.translate_vim_effects(effects))
    }

    fn navigate_diagnostic(&mut self, forward: bool, count: usize) -> Vec<Effect> {
        let diagnostics = self.diagnostics();
        if diagnostics.is_empty() {
            return Vec::new();
        }
        let offset = self.live.cursor_byte_offset();
        let containing = diagnostics
            .iter()
            .position(|diagnostic| diagnostic.range.contains(&offset));
        let steps = count % diagnostics.len();
        let index = if let Some(index) = containing {
            if forward {
                (index + steps) % diagnostics.len()
            } else {
                (index + diagnostics.len() - steps) % diagnostics.len()
            }
        } else if forward {
            let first = diagnostics
                .iter()
                .position(|diagnostic| diagnostic.range.start > offset)
                .unwrap_or(0);
            (first + count.saturating_sub(1)) % diagnostics.len()
        } else {
            let first = diagnostics
                .iter()
                .rposition(|diagnostic| diagnostic.range.start < offset)
                .unwrap_or(diagnostics.len() - 1);
            (first + diagnostics.len() - count.saturating_sub(1) % diagnostics.len())
                % diagnostics.len()
        };
        let target = diagnostics[index].range.start;
        self.jump_to_offset(target).unwrap_or_default()
    }

    fn handle_rendered_navigation_key(&mut self, key: KeyInput) -> Vec<Effect> {
        if let KeyCodeKind::Char(c) = key.code.kind {
            if c.is_ascii_digit()
                && key.mods == Modifiers::default()
                && (c != '0' || self.rendered_state.count > 0)
            {
                let digit = c.to_digit(10).unwrap() as usize;
                self.rendered_state.count = self.rendered_state.count.saturating_mul(10) + digit;
                return Vec::new();
            }
        }

        let cursor = self.rendered_state.cursor;
        let search = self.rendered_state.search.current().cloned();
        let count = std::mem::take(&mut self.rendered_state.count);
        if self.rendered_state.layout_cache.is_none() {
            if let Some(rows) = self.rendered_state.row_cache.as_mut() {
                if let Some(row) = nav::vertical_target(key, &cursor, rows.row_count(), count) {
                    self.rendered_state.cursor = rows.cursor_for_row(
                        row,
                        cursor.desired_column,
                        self.live.rendered_model(),
                        self.live.text_ref(),
                        self.live.highlighter(),
                        self.rendered_state.last_width,
                    );
                    self.commit_rendered_cursor();
                    self.refresh_character_selection();
                    return vec![Effect::CursorMoved];
                }
                if let Some(forward) = nav::horizontal_direction(key) {
                    let point = nav::horizontal_point_with(
                        &cursor,
                        rows.row_count(),
                        forward,
                        count.max(1),
                        |row| {
                            rows.row_current(
                                row,
                                self.live.rendered_model(),
                                self.live.text_ref(),
                                self.live.highlighter(),
                                self.rendered_state.last_width,
                            )
                        },
                    );
                    if let Some(point) = point {
                        self.rendered_state.cursor = RenderedCursor::at(point);
                        self.commit_rendered_cursor();
                        self.refresh_character_selection();
                        return vec![Effect::CursorMoved];
                    }
                    return Vec::new();
                }
                if let Some(motion) = nav::word_motion(key) {
                    let point = nav::word_point_with(
                        &cursor,
                        rows.row_count(),
                        self.live.text_ref(),
                        motion,
                        count.max(1),
                        |row| {
                            rows.row_current(
                                row,
                                self.live.rendered_model(),
                                self.live.text_ref(),
                                self.live.highlighter(),
                                self.rendered_state.last_width,
                            )
                        },
                    );
                    if let Some(point) = point {
                        self.rendered_state.cursor = RenderedCursor::at(point);
                        self.commit_rendered_cursor();
                        self.refresh_character_selection();
                        return vec![Effect::CursorMoved];
                    }
                    return Vec::new();
                }
                if let Some(target) =
                    nav::jump_row_for_key(key, &cursor, count, &rows.jump_targets())
                {
                    if let Some(row) = target {
                        self.rendered_state.cursor = rows.cursor_for_row(
                            row,
                            cursor.desired_column,
                            self.live.rendered_model(),
                            self.live.text_ref(),
                            self.live.highlighter(),
                            self.rendered_state.last_width,
                        );
                        self.commit_rendered_cursor();
                        self.refresh_character_selection();
                        return vec![Effect::CursorMoved];
                    }
                    return Vec::new();
                }
                if key.mods == Modifiers::default() {
                    if let KeyCodeKind::Char(direction @ ('/' | '?')) = key.code.kind {
                        let mut draft = RenderedSearch::new("");
                        draft.set_direction(if direction == '/' {
                            SearchDirection::Forward
                        } else {
                            SearchDirection::Backward
                        });
                        self.rendered_state.search.begin(draft, cursor);
                        return Vec::new();
                    }
                    if let KeyCodeKind::Char('n' | 'N') = key.code.kind {
                        let Some(search) = search.filter(|search| !search.pattern.is_empty())
                        else {
                            return Vec::new();
                        };
                        let direction = if key.code.kind == KeyCodeKind::Char('n') {
                            search.direction()
                        } else if search.direction() == SearchDirection::Forward {
                            SearchDirection::Backward
                        } else {
                            SearchDirection::Forward
                        };
                        let target = rows.find_next_match(
                            &search,
                            &cursor,
                            direction,
                            self.live.rendered_model(),
                            self.live.highlighter(),
                            self.rendered_state.last_width,
                        );
                        self.rendered_state.search.replace_last(search);
                        if let Some(row) = target {
                            let wrapped = if direction == SearchDirection::Forward {
                                row <= cursor.line
                            } else {
                                row >= cursor.line
                            };
                            self.rendered_state.cursor = rows.cursor_for_row(
                                row,
                                cursor.desired_column,
                                self.live.rendered_model(),
                                self.live.text_ref(),
                                self.live.highlighter(),
                                self.rendered_state.last_width,
                            );
                            self.commit_rendered_cursor();
                            self.refresh_character_selection();
                            let mut effects = vec![Effect::CursorMoved];
                            if wrapped {
                                effects.push(Effect::Message {
                                    text: " (wrapped)".to_string(),
                                    severity: Severity::Info,
                                });
                            }
                            return effects;
                        }
                        return Vec::new();
                    }
                    if let KeyCodeKind::Char('0' | '^' | '$') = key.code.kind {
                        let line = rows.row_current(
                            cursor.line,
                            self.live.rendered_model(),
                            self.live.text_ref(),
                            self.live.highlighter(),
                            self.rendered_state.last_width,
                        );
                        if let Some(point) = line.as_ref().and_then(|line| {
                            nav::edge_point_with(
                                cursor.line,
                                key.code.kind == KeyCodeKind::Char('$'),
                                line,
                            )
                        }) {
                            self.rendered_state.cursor = RenderedCursor::at(point);
                            self.commit_rendered_cursor();
                            self.refresh_character_selection();
                            return vec![Effect::CursorMoved];
                        }
                        return Vec::new();
                    }
                    let target_row = match key.code.kind {
                        KeyCodeKind::Char('{') => rows.boundary_row(cursor.line, false),
                        KeyCodeKind::Char('}') => rows.boundary_row(cursor.line, true),
                        KeyCodeKind::Home => rows.edge_content_row(false),
                        KeyCodeKind::End => rows.edge_content_row(true),
                        _ => None,
                    };
                    if let Some(row) = target_row {
                        self.rendered_state.cursor = rows.cursor_for_row(
                            row,
                            cursor.desired_column,
                            self.live.rendered_model(),
                            self.live.text_ref(),
                            self.live.highlighter(),
                            self.rendered_state.last_width,
                        );
                        self.commit_rendered_cursor();
                        self.refresh_character_selection();
                        return vec![Effect::CursorMoved];
                    }
                    if matches!(key.code.kind, KeyCodeKind::Enter) {
                        return rows.target_message_at_row(cursor.line).map_or_else(
                            Vec::new,
                            |text| {
                                vec![Effect::Message {
                                    text,
                                    severity: Severity::Info,
                                }]
                            },
                        );
                    }
                }
                if key.mods == Modifiers::default() && key.code.kind == KeyCodeKind::Char('z') {
                    self.render_layout(self.rendered_state.last_width);
                } else {
                    return Vec::new();
                }
            }
        }
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return Vec::new();
        };
        let text = nav::key_inspects_source(key).then(|| self.live.text());
        let result = nav::handle_key(
            key,
            &cursor,
            search.as_ref(),
            layout.lines.len(),
            &layout.jump_targets,
            layout,
            count,
            text.as_deref().unwrap_or_default(),
        );
        let mut effects = Vec::new();
        if result.search_changed {
            if let Some(new_search) = result.new_search {
                if new_search.pattern.is_empty() {
                    self.rendered_state.search.begin(new_search, cursor);
                } else {
                    self.rendered_state.search.replace_last(new_search);
                }
            } else {
                self.rendered_state.search.clear();
            }
        }
        if let Some(new_cursor) = result.new_cursor.filter(|_| result.cursor_moved) {
            self.rendered_state.cursor = new_cursor;
            self.commit_rendered_cursor();
            self.refresh_character_selection();
            effects.push(Effect::CursorMoved);
        }
        let collapse_hides_selection = result.fm_collapsed_toggled
            && !self.rendered_state.fm_collapsed
            && self.mode() == Mode::Select
            && crate::frontmatter::front_matter_span(
                text.as_deref()
                    .unwrap_or_else(|| self.live.highlighter().text()),
            )
            .is_some_and(|front_matter| {
                self.rendered_selection().is_some_and(|selection| {
                    selection.source_ranges.iter().any(|source| {
                        source.start < front_matter.end && front_matter.start < source.end
                    })
                })
            });
        if result.layout_dirty {
            self.rendered_state.invalidate();
        }
        if result.fm_collapsed_toggled {
            self.rendered_state.fm_collapsed = !self.rendered_state.fm_collapsed;
        }
        if let Some(message) = result.message {
            effects.push(Effect::Message {
                text: message,
                severity: Severity::Info,
            });
        }
        if collapse_hides_selection {
            return self.finish_select(Mode::Normal, effects);
        }
        effects
    }

    /// Handle pattern entry while a rendered search prompt is active.
    fn handle_rendered_search_input(&mut self, key: KeyInput) -> Vec<Effect> {
        match key.code.kind {
            KeyCodeKind::Esc => {
                self.rendered_state.search.cancel();
                Vec::new()
            }
            KeyCodeKind::Enter => {
                self.rendered_state.search.submit();
                Vec::new()
            }
            KeyCodeKind::Char(c)
                if !key.mods.ctrl
                    && !key.mods.alt
                    && !key.mods.shift
                    && (c.is_ascii_alphanumeric() || c == ' ' || c == '.' || c == '_') =>
            {
                let Some((_, cursor)) = self.rendered_state.search.prompt() else {
                    return Vec::new();
                };
                let cursor = *cursor;
                let Some((search_state, _)) = self.rendered_state.search.prompt() else {
                    return Vec::new();
                };
                let mut search_state = search_state.clone();
                search_state.pattern.push(c);
                let match_line = if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
                    nav::find_next_match(
                        &search_state,
                        &cursor,
                        layout,
                        self.live.text_ref(),
                        search_state.direction(),
                    )
                } else if let Some(rows) = self.rendered_state.row_cache.as_mut() {
                    rows.find_next_match(
                        &search_state,
                        &cursor,
                        search_state.direction(),
                        self.live.rendered_model(),
                        self.live.highlighter(),
                        self.rendered_state.last_width,
                    )
                } else {
                    None
                };
                self.rendered_state
                    .search
                    .update_draft(|draft| *draft = search_state);
                if let Some(match_line) = match_line {
                    self.rendered_state.cursor =
                        if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
                            nav::cursor_for_row(
                                match_line,
                                self.rendered_state.cursor.desired_column,
                                layout,
                            )
                        } else {
                            self.rendered_state
                                .row_cache
                                .as_mut()
                                .unwrap()
                                .cursor_for_row(
                                    match_line,
                                    self.rendered_state.cursor.desired_column,
                                    self.live.rendered_model(),
                                    self.live.text_ref(),
                                    self.live.highlighter(),
                                    self.rendered_state.last_width,
                                )
                        };
                    self.commit_rendered_cursor();
                    self.refresh_character_selection();
                    vec![Effect::CursorMoved]
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    fn enter_insert_from_rendered(&mut self, action: RenderedExitAction) -> Vec<Effect> {
        self.commit_rendered_cursor();
        if self.rendered_state.layout_cache.is_some() {
            self.ensure_rendered_rows(self.rendered_state.last_width);
        }
        self.rendered_state.search.cancel();
        self.rendered_state.count = 0;
        let vim_effects = self.live.handle_key(action.key());
        self.translate_vim_effects(vim_effects)
    }

    fn enter_select(&mut self, shape: SelectionShape) -> Vec<Effect> {
        let point = self.rendered_state.cursor.point();
        let endpoint = self.selection_endpoint_at(point);
        self.rendered_state.register_input = RegisterInput::Default;
        self.session_mode = SessionMode::Select(ActiveSelection {
            anchor: endpoint.clone(),
            active: endpoint,
            kind: SelectionKind::from_shape(shape),
        });
        self.refresh_character_selection();
        vec![Effect::ModeChanged(Mode::Select)]
    }

    fn switch_or_cancel_selection_shape(&mut self, shape: SelectionShape) -> Vec<Effect> {
        let current = match &self.session_mode {
            SessionMode::Select(selection) => selection.kind.shape(),
            SessionMode::CoreDriven | SessionMode::Command(_) => return Vec::new(),
        };
        if current == shape {
            self.finish_select(Mode::Normal, Vec::new())
        } else {
            if let SessionMode::Select(selection) = &mut self.session_mode {
                selection.switch_kind(shape);
            }
            self.refresh_character_selection();
            vec![Effect::CursorMoved]
        }
    }

    fn apply_select_operator(
        &mut self,
        operator: RangeOperator,
        publication: YankPublication,
    ) -> Vec<Effect> {
        if operator != RangeOperator::Yank
            && matches!(&self.session_mode, SessionMode::Select(active) if active.kind.shape() == SelectionShape::Line)
        {
            return self.apply_rendered_line_operator(operator, publication);
        }
        if let Some(effects) = self.try_apply_bounded_character_operator(operator, publication) {
            return effects;
        }
        if self.rendered_state.layout_cache.is_none() && self.rendered_state.row_cache.is_some() {
            self.render_layout(self.rendered_state.last_width);
        }
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return Vec::new();
        };
        let SessionMode::Select(active) = &self.session_mode else {
            return Vec::new();
        };
        let text = self.live.text();
        let mut selection = nav::project_selection_from_source_positions(
            active.anchor.point,
            active.active.point,
            active.kind.shape(),
            active.anchor.source,
            active.active.source,
            layout,
            &text,
        );
        if let SelectionKind::Character { ranges } = &active.kind {
            selection.source_ranges = ranges.clone();
            selection.rows = nav::character_selection_rows(ranges, layout);
        }
        if operator == RangeOperator::Yank {
            selection = prepare_fenced_code_yank_selection(
                selection,
                layout,
                self.live.highlighter().text(),
                &self.rendered_state.code_fence_regions,
            );
        } else if matches!(operator, RangeOperator::Delete | RangeOperator::Change)
            && selection.shape == SelectionShape::Character
        {
            selection.source_ranges =
                nav::character_mutation_ranges(&selection.source_ranges, layout, &text);
        }
        if selection.source_ranges.is_empty() {
            return Vec::new();
        }
        let clipboard_content = if matches!(operator, RangeOperator::Delete | RangeOperator::Change)
            && selection.shape == SelectionShape::Character
        {
            crate::clipboard::ClipboardContent::from_markdown(
                selection
                    .source_ranges
                    .iter()
                    .filter_map(|range| text.get(range.clone()))
                    .collect(),
            )
        } else {
            crate::clipboard::rendered_selection_content(&selection, layout, &text)
        };
        let register = self.rendered_state.register_input.take();
        let projected = project_selection_for_vim(selection);
        self.ensure_rendered_rows(self.rendered_state.last_width);
        let vim_effects = if operator == RangeOperator::Yank {
            self.live.apply_yank(
                ProjectedYank {
                    selection: projected,
                    payload: clipboard_content.markdown().to_string(),
                },
                register,
            )
        } else {
            self.live.apply_selection(projected, operator, register)
        };
        let clipboard_output = match publication {
            YankPublication::Configured => clipboard_content.clone(),
            YankPublication::PlainText => crate::clipboard::ClipboardContent::invariant(
                clipboard_content.plain_text().to_string(),
            ),
        };
        let effects =
            self.translate_vim_effects_with_clipboard(vim_effects, Some(&clipboard_output));
        self.remap_active_cursor_from_canonical();
        let target_mode = if operator == RangeOperator::Change {
            Mode::Insert
        } else {
            Mode::Normal
        };
        self.finish_select(target_mode, effects)
    }

    fn try_apply_bounded_character_operator(
        &mut self,
        operator: RangeOperator,
        publication: YankPublication,
    ) -> Option<Vec<Effect>> {
        if !matches!(operator, RangeOperator::Delete | RangeOperator::Change)
            || (self.rendered_state.row_cache.is_none()
                && self.rendered_state.layout_cache.is_none())
        {
            return None;
        }
        let SessionMode::Select(active) = &self.session_mode else {
            return None;
        };
        if !matches!(active.kind, SelectionKind::Character { .. }) {
            return None;
        }
        #[cfg(test)]
        let mut stage = std::time::Instant::now();
        let lines = self.selection_lines(active.anchor.point, active.active.point);
        let selection = self.rendered_selection()?;
        let first = selection.source_ranges.first()?;
        let last = selection.source_ranges.last()?;
        let in_fence = self.live.rendered_model().blocks.iter().any(|block| {
            matches!(&block.kind, crate::rendered::BlockKind::CodeFence {
                content_span,
                indented: false,
                ..
            } if content_span.start < first.start && last.end < content_span.end)
        });
        #[cfg(test)]
        {
            self.rendered_state.last_select_operator_profile_ns[0] = stage.elapsed().as_nanos();
            stage = std::time::Instant::now();
        }
        let coverage = if in_fence {
            lines
        } else if let Some(rows) = self.rendered_state.row_cache.as_ref() {
            rows.rows_near_source_range(
                &(first.start..last.end),
                self.live.rendered_model(),
                self.live.highlighter(),
                self.rendered_state.last_width,
            )
        } else {
            let layout = self.rendered_state.layout_cache.as_ref()?;
            let model = self.live.rendered_model();
            let first_block = model
                .blocks
                .partition_point(|block| block.span.end <= first.start);
            let after_block = model
                .blocks
                .partition_point(|block| block.span.start < last.end);
            let start_block = first_block.saturating_sub(1);
            let end_block = after_block.saturating_add(1).min(model.blocks.len());
            let start_row = self
                .rendered_state
                .layout_boundaries
                .get(start_block)?
                .rows
                .start;
            let end_row = self
                .rendered_state
                .layout_boundaries
                .get(end_block.saturating_sub(1))?
                .rows
                .end;
            (start_row..end_row)
                .filter_map(|row| layout.lines.get(row).cloned().map(|line| (row, line)))
                .collect()
        };
        if coverage.len() > 512 {
            return None;
        }
        #[cfg(test)]
        {
            self.rendered_state.last_select_operator_profile_ns[1] = stage.elapsed().as_nanos();
            stage = std::time::Instant::now();
        }
        let text = self.live.text_ref();
        let ranges = nav::character_mutation_ranges_with_lines(
            &selection.source_ranges,
            coverage.iter().map(|(_, line)| line),
            text,
        );
        if ranges.is_empty() {
            return None;
        }
        let clipboard_content = crate::clipboard::ClipboardContent::from_markdown(
            ranges
                .iter()
                .filter_map(|range| text.get(range.clone()))
                .collect(),
        );
        let register = self.rendered_state.register_input.take();
        self.ensure_rendered_rows(self.rendered_state.last_width);
        #[cfg(test)]
        {
            self.rendered_state.last_select_operator_profile_ns[2] = stage.elapsed().as_nanos();
            stage = std::time::Instant::now();
        }
        let vim_effects =
            self.live
                .apply_selection(ProjectedSelection::Character { ranges }, operator, register);
        #[cfg(test)]
        {
            self.rendered_state.last_select_operator_profile_ns[3] = stage.elapsed().as_nanos();
            stage = std::time::Instant::now();
        }
        let clipboard_output = match publication {
            YankPublication::Configured => clipboard_content.clone(),
            YankPublication::PlainText => crate::clipboard::ClipboardContent::invariant(
                clipboard_content.plain_text().to_string(),
            ),
        };
        let effects =
            self.translate_vim_effects_with_clipboard(vim_effects, Some(&clipboard_output));
        #[cfg(test)]
        {
            self.rendered_state.last_select_operator_profile_ns[4] = stage.elapsed().as_nanos();
            stage = std::time::Instant::now();
        }
        if self.rendered_state.row_cache.is_none() {
            self.remap_active_cursor_from_canonical();
        }
        let target_mode = if operator == RangeOperator::Change {
            Mode::Insert
        } else {
            Mode::Normal
        };
        let result = self.finish_select(target_mode, effects);
        #[cfg(test)]
        {
            self.rendered_state.last_select_operator_profile_ns[5] = stage.elapsed().as_nanos();
        }
        Some(result)
    }

    fn apply_rendered_line_operator(
        &mut self,
        operator: RangeOperator,
        publication: YankPublication,
    ) -> Vec<Effect> {
        let SessionMode::Select(active) = &self.session_mode else {
            return Vec::new();
        };
        let first = active.anchor.source.0.min(active.active.source.0);
        let last = active.anchor.source.0.max(active.active.source.0);
        let line_starts = self.live.highlighter().line_starts();
        let start = line_starts
            .get(first)
            .copied()
            .unwrap_or(self.live.text_ref().len());
        let end = line_starts
            .get(last + 1)
            .copied()
            .unwrap_or(self.live.text_ref().len());
        if start >= end {
            return Vec::new();
        }
        let clipboard_content = crate::clipboard::ClipboardContent::from_markdown(
            self.live.text_ref()[start..end].to_string(),
        );
        let register = self.rendered_state.register_input.take();
        self.ensure_rendered_rows(self.rendered_state.last_width);
        let projected = ProjectedSelection::Line {
            ranges: std::iter::once(start..end).collect(),
        };
        let vim_effects = if operator == RangeOperator::Yank {
            self.live.apply_yank(
                ProjectedYank {
                    selection: projected,
                    payload: clipboard_content.markdown().to_string(),
                },
                register,
            )
        } else {
            self.live.apply_selection(projected, operator, register)
        };
        let clipboard_output = match publication {
            YankPublication::Configured => clipboard_content.clone(),
            YankPublication::PlainText => crate::clipboard::ClipboardContent::invariant(
                clipboard_content.plain_text().to_string(),
            ),
        };
        let effects =
            self.translate_vim_effects_with_clipboard(vim_effects, Some(&clipboard_output));
        self.remap_active_cursor_from_canonical();
        let target_mode = if operator == RangeOperator::Change {
            Mode::Insert
        } else {
            Mode::Normal
        };
        self.finish_select(target_mode, effects)
    }

    fn finish_select(&mut self, mode: Mode, mut effects: Vec<Effect>) -> Vec<Effect> {
        self.rendered_state.register_input = RegisterInput::Default;
        self.rendered_state.search.cancel();
        self.session_mode = SessionMode::CoreDriven;
        effects.retain(|effect| !matches!(effect, Effect::ModeChanged(_)));
        effects.push(Effect::ModeChanged(mode));
        effects
    }

    /// Recompute the displayed endpoint from the canonical source cursor.
    ///
    /// Line-range operations move the wrapped Vim cursor even when the
    /// document is unchanged (notably yank), so the cached rendered endpoint
    /// must be updated before returning to Normal.
    fn remap_active_cursor_from_canonical(&mut self) {
        if self.rendered_state.layout_cache.is_none() {
            let offset = self.live.cursor_byte_offset();
            let indexed = if let Some(rows) = self.rendered_state.row_cache.as_mut() {
                let model = self.live.rendered_model();
                let candidate = model
                    .blocks
                    .partition_point(|block| block.span.start <= offset)
                    .saturating_sub(1);
                [candidate, candidate.saturating_add(1)]
                    .into_iter()
                    .find_map(|index| {
                        rows.point_for_offset_in_block(
                            index,
                            offset,
                            model,
                            self.live.text_ref(),
                            self.live.highlighter(),
                            self.rendered_state.last_width,
                        )
                    })
            } else {
                None
            };
            if let Some(cursor) = indexed {
                self.rendered_state.cursor = cursor;
                return;
            }
            if offset == self.live.text_ref().len() {
                if let Some(rows) = self.rendered_state.row_cache.as_ref() {
                    self.rendered_state.cursor =
                        RenderedCursor::new(rows.row_count().saturating_sub(1));
                    return;
                }
            }
            if self.rendered_state.row_cache.is_some() {
                self.render_layout(self.rendered_state.last_width);
            }
        }
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let source = self.live.cursor();
        self.rendered_state.cursor = nav::enter_rendered_at_offset(
            source.0,
            self.live.cursor_byte_offset(),
            layout,
            |offset| nav::source_line_for_offset(self.live.highlighter().line_starts(), offset),
            |offset| self.live.byte_before_is_newline(offset),
        );
    }

    fn commit_rendered_cursor(&mut self) {
        let source_offset = if let Some(layout) = self.rendered_state.layout_cache.as_ref() {
            nav::canonical_source_offset_for_row(
                &self.rendered_state.cursor,
                self.live.cursor_byte_offset(),
                layout,
            )
        } else if let Some(rows) = self.rendered_state.row_cache.as_mut() {
            let Some(line) = rows.row_current(
                self.rendered_state.cursor.line,
                self.live.rendered_model(),
                self.live.text_ref(),
                self.live.highlighter(),
                self.rendered_state.last_width,
            ) else {
                return;
            };
            nav::canonical_source_offset_for_line(
                &self.rendered_state.cursor,
                self.live.cursor_byte_offset(),
                &line,
            )
        } else {
            return;
        };
        let source = self.live.position_for_byte_offset(source_offset);
        self.live.jump_to(source.0, source.1);
    }

    fn refresh_character_selection(&mut self) {
        let SessionMode::Select(selection) = &self.session_mode else {
            return;
        };
        let point = self.rendered_state.cursor.point();
        let anchor = selection.anchor.clone();
        let shape = selection.kind.shape();
        let active = self.selection_endpoint_at(point);
        let ranges = (shape == SelectionShape::Character).then(|| {
            nav::project_selection_from_rows(
                anchor.point,
                active.point,
                SelectionShape::Character,
                (anchor.source, active.source),
                &self.selection_lines(anchor.point, active.point),
                None,
                self.live.text_ref(),
            )
            .source_ranges
        });
        if let SessionMode::Select(selection) = &mut self.session_mode {
            selection.active = active;
            if let (SelectionKind::Character { ranges: selected }, Some(ranges)) =
                (&mut selection.kind, ranges)
            {
                *selected = ranges;
            }
        }
    }

    fn translate_vim_effects(
        &mut self,
        vim_effects: impl IntoIterator<Item = VimEffect>,
    ) -> Vec<Effect> {
        self.translate_vim_effects_with_clipboard(vim_effects, None)
    }

    fn translate_vim_effects_with_clipboard(
        &mut self,
        vim_effects: impl IntoIterator<Item = VimEffect>,
        clipboard_override: Option<&crate::clipboard::ClipboardContent>,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut left_insert = false;
        let mut edited = false;
        for effect in vim_effects {
            match effect {
                VimEffect::ModeChanged(vim_mode) => {
                    let mode = if vim_mode == crate::vim::Mode::Insert {
                        Mode::Insert
                    } else {
                        Mode::Normal
                    };
                    left_insert |= mode == Mode::Normal;
                    effects.push(Effect::ModeChanged(mode));
                }
                VimEffect::Edited { .. } => {
                    edited = true;
                    effects.push(Effect::Edited);
                }
                VimEffect::CursorMoved => effects.push(Effect::CursorMoved),
                VimEffect::ExCommand { command } => {
                    effects.extend(self.process_ex_command(&command))
                }
                VimEffect::CommandCancelled => {}
                VimEffect::ClipboardYank(text) => effects.push(Effect::ClipboardWrite(
                    clipboard_override
                        .cloned()
                        .unwrap_or_else(|| crate::clipboard::ClipboardContent::from_markdown(text)),
                )),
                VimEffect::SearchWrapped => effects.push(Effect::Message {
                    text: "Search wrapped around buffer".to_string(),
                    severity: Severity::Info,
                }),
                VimEffect::Bell => {}
            }
        }
        if edited {
            self.publish_rendered_text_change();
        }
        if left_insert && !edited {
            // Insert-mode motions are owned by the canonical Vim cursor.
            // Re-enter rendered coordinates from the retained rows before
            // the next rendered motion.
            self.remap_active_cursor_from_canonical();
        }
        effects
    }

    fn publish_rendered_text_change(&mut self) {
        #[cfg(test)]
        {
            self.rendered_state.last_publish_profile_ns = [0; 3];
            self.rendered_state.last_publish_used_retained = false;
        }
        let change = self.live.take_projection_change();
        self.rendered_state.layout_cache = None;
        self.rendered_state.layout_boundaries.clear();
        let Some(rows) = &mut self.rendered_state.row_cache else {
            self.rendered_state.row_cache = None;
            self.rendered_state.code_fence_regions.clear();
            return;
        };
        #[cfg(test)]
        let started = std::time::Instant::now();
        let (index, updated) = match change {
            PendingProjectionChange::One(ModelChange::Local {
                index,
                source_delta,
                line_delta,
                old_links,
                new_links,
            }) => {
                let updated = rows.block_link_count(index) == Some(old_links)
                    && rows.replace_local_block(
                        index,
                        self.live.rendered_model(),
                        self.live.text_ref(),
                        self.live.highlighter(),
                        self.rendered_state.last_width,
                        crate::rendered::RowShift {
                            source: source_delta,
                            lines: line_delta,
                        },
                    )
                    && rows.block_link_count(index) == Some(new_links);
                (index, updated)
            }
            PendingProjectionChange::One(ModelChange::Window {
                old_blocks,
                new_blocks,
                old_source_end,
                source_delta,
                line_delta,
                old_links,
                new_links,
            }) => {
                let updated = rows.replace_window(
                    crate::rendered::RowWindowChange {
                        old_blocks,
                        new_blocks: new_blocks.clone(),
                        old_source_end,
                        shift: crate::rendered::RowShift {
                            source: source_delta,
                            lines: line_delta,
                        },
                        old_links,
                        new_links,
                    },
                    self.live.rendered_model(),
                    self.live.highlighter(),
                    self.rendered_state.last_width,
                );
                let cursor = self.live.cursor_byte_offset();
                let first_candidate = self
                    .live
                    .rendered_model()
                    .blocks
                    .partition_point(|block| block.span.end <= cursor);
                let index = first_candidate.clamp(new_blocks.start, new_blocks.end - 1);
                (index, updated)
            }
            PendingProjectionChange::One(ModelChange::FenceInterior {
                index,
                edit_range,
                new_text,
                source_delta,
                line_delta,
            }) => {
                let edit = crate::vim::TextEdit {
                    range: edit_range,
                    new_text_len: new_text.len(),
                    new_text,
                };
                let updated = rows.splice_fence_interior(
                    index,
                    &edit,
                    self.live.rendered_model(),
                    self.live.highlighter(),
                    crate::rendered::RowShift {
                        source: source_delta,
                        lines: line_delta,
                    },
                );
                (index, updated)
            }
            PendingProjectionChange::None
            | PendingProjectionChange::Multiple
            | PendingProjectionChange::One(ModelChange::Wide) => (0, false),
        };
        #[cfg(test)]
        {
            self.rendered_state.last_publish_profile_ns[0] = started.elapsed().as_nanos();
        }
        if !updated {
            self.rendered_state.row_cache = None;
            self.rendered_state.code_fence_regions.clear();
            return;
        }
        #[cfg(test)]
        let started = std::time::Instant::now();
        let Some(cursor) = rows.point_for_offset_in_block(
            index,
            self.live.cursor_byte_offset(),
            self.live.rendered_model(),
            self.live.text_ref(),
            self.live.highlighter(),
            self.rendered_state.last_width,
        ) else {
            self.rendered_state.row_cache = None;
            self.rendered_state.code_fence_regions.clear();
            return;
        };
        #[cfg(test)]
        {
            self.rendered_state.last_publish_profile_ns[1] = started.elapsed().as_nanos();
        }
        self.rendered_state.cursor = cursor;
        #[cfg(test)]
        let started = std::time::Instant::now();
        self.rendered_state.code_fence_regions = rows.fence_regions();
        #[cfg(test)]
        {
            self.rendered_state.last_publish_profile_ns[2] = started.elapsed().as_nanos();
            self.rendered_state.last_publish_used_retained = true;
        }
    }

    /// Process an ex command text and produce effects.
    ///
    /// Handles: :w, :w!, :w {path}, :wq, :x, :q, :q!, :e, :e!, :e {path},
    /// :saveas, :{number}, :s, :noh, :help, :set, and unknown commands.
    fn process_ex_command(&mut self, command: &str) -> Vec<Effect> {
        let cmd = command.trim();
        let (base, args) = Self::parse_ex_command(cmd);

        match base {
            "w" | "wq" | "x" => {
                let force = args.1;
                if base == "w" && args.0.is_some() {
                    // :w {path} — save copy without retargeting
                    vec![Effect::SaveRequested {
                        path: args.0.map(std::path::PathBuf::from),
                        force: args.1,
                        retarget: false,
                        then_quit: false,
                    }]
                } else {
                    vec![Effect::SaveRequested {
                        path: None,
                        force,
                        retarget: false,
                        then_quit: base != "w",
                    }]
                }
            }
            "q" => vec![Effect::QuitRequested { force: args.1 }],
            "e" => match args.0 {
                Some(path) => vec![Effect::OpenRequested {
                    path: std::path::PathBuf::from(path),
                    force: args.1,
                }],
                None => vec![Effect::ReloadCurrentRequested { force: args.1 }],
            },
            "reload" | "reload-all" if args.0.is_none() && !args.1 => {
                if base == "reload" {
                    vec![Effect::ReloadCurrentRequested { force: true }]
                } else {
                    vec![Effect::ReloadAllRequested]
                }
            }
            "reload" | "reload-all" => vec![Effect::Message {
                text: format!("Unexpected arguments for :{base}"),
                severity: Severity::Warning,
            }],
            "saveas" => vec![Effect::SaveRequested {
                path: args.0.map(std::path::PathBuf::from),
                force: false,
                retarget: true,
                then_quit: false,
            }],
            _ if args.0.is_none()
                && !base.is_empty()
                && base.chars().all(|c| c.is_ascii_digit()) =>
            {
                // :{number} — jump to a 1-based line, clamped at EOF.
                match base.parse::<usize>() {
                    Ok(0) | Err(_) => vec![Effect::Message {
                        text: format!("Invalid line number: {}", base),
                        severity: Severity::Warning,
                    }],
                    Ok(line) => {
                        let row = line.min(self.line_count()) - 1;
                        self.live.jump_to(row, 0);
                        self.remap_rendered_cursor(row, 0);
                        vec![Effect::CursorMoved]
                    }
                }
            }
            "s" | "substitute" => {
                // :[range]s/pattern/replacement/[flags]
                let Some(substitute_args) = args.0 else {
                    return vec![Effect::Message {
                        text: "Invalid substitute command".to_string(),
                        severity: Severity::Warning,
                    }];
                };
                let Some((start_row, end_row)) =
                    Self::parse_substitute_range(cmd, self.cursor().0, self.line_count())
                else {
                    return vec![Effect::Message {
                        text: "Invalid substitute range".to_string(),
                        severity: Severity::Warning,
                    }];
                };
                match self.live.substitute(substitute_args, start_row, end_row) {
                    Ok(outcome) if outcome.effects.is_empty() => vec![Effect::Message {
                        text: "No replacement done".to_string(),
                        severity: Severity::Info,
                    }],
                    Ok(outcome) => self.translate_vim_effects(outcome),
                    Err(_) => vec![Effect::Message {
                        text: "Invalid substitute command".to_string(),
                        severity: Severity::Warning,
                    }],
                }
            }
            "noh" => {
                self.live.clear_search_highlight();
                self.rendered_state.search.clear();
                vec![Effect::Message {
                    text: "Search highlighting cleared".to_string(),
                    severity: Severity::Info,
                }]
            }
            "help" => vec![Effect::HelpRequested],
            "set" => match args.0 {
                Some("wrap") => vec![Effect::SetWrap(true)],
                Some("nowrap") => vec![Effect::SetWrap(false)],
                Some("spell") => {
                    self.set_spell_enabled(true);
                    vec![Effect::Message {
                        text: "Spell checking enabled".to_string(),
                        severity: Severity::Info,
                    }]
                }
                Some("nospell") => {
                    self.set_spell_enabled(false);
                    vec![Effect::Message {
                        text: "Spell checking disabled".to_string(),
                        severity: Severity::Info,
                    }]
                }
                Some(unknown) => vec![Effect::Message {
                    text: format!("Unknown option: {unknown}"),
                    severity: Severity::Warning,
                }],
                None => vec![Effect::Message {
                    text: "Usage: :set <option>".to_string(),
                    severity: Severity::Warning,
                }],
            },
            "qa" => vec![Effect::QuitAllRequested { force: args.1 }],
            "tabnew" => {
                if let Some(path) = args.0 {
                    vec![Effect::TabNewRequested {
                        path: std::path::PathBuf::from(path),
                    }]
                } else {
                    vec![Effect::Message {
                        text: ":tabnew requires a file path".to_string(),
                        severity: Severity::Warning,
                    }]
                }
            }
            "tabclose" => vec![Effect::TabCloseRequested {
                index: None,
                force: args.1,
            }],
            _ => vec![Effect::Message {
                text: format!("Unknown command: {}", base),
                severity: Severity::Warning,
            }],
        }
    }

    /// Parse an ex command into (base_command, (path_arg, force_flag)).
    fn parse_ex_command(cmd: &str) -> (&str, (Option<&str>, bool)) {
        let cmd = cmd.trim_start_matches(':').trim_start();

        // Only a leading range can introduce substitute syntax. Path arguments
        // may contain the same bytes and must remain literal.
        let (base, rest_str) = if let Some((base, _, args)) = Self::substitute_parts(cmd) {
            (base, Some(args))
        } else {
            let mut parts = cmd.splitn(2, char::is_whitespace);
            let b = parts.next().unwrap_or(cmd);
            (b, parts.next())
        };

        // Check for ! suffix on base command
        let (base, force) = if let Some(stripped) = base.strip_suffix('!') {
            (stripped, true)
        } else {
            (base, false)
        };

        let path = rest_str.and_then(|a| {
            let a = a.trim();
            if a.is_empty() {
                None
            } else {
                Some(a)
            }
        });

        (base, (path, force))
    }

    fn substitute_parts(command: &str) -> Option<(&'static str, &str, &str)> {
        for (prefix, base) in [("substitute/", "substitute"), ("s/", "s")] {
            if let Some(position) = command.find(prefix) {
                let range = &command[..position];
                let valid_range = range.is_empty()
                    || range == "%"
                    || range
                        .chars()
                        .all(|character| character.is_ascii_digit() || character == ',')
                        && range.split(',').all(|part| !part.is_empty())
                        && range.matches(',').count() <= 1;
                if valid_range {
                    return Some((base, range, &command[position + base.len()..]));
                }
            }
        }
        None
    }

    /// Resolve a substitute command's optional 1-based line range.
    /// An omitted range targets the cursor line; `%` targets the whole buffer.
    fn parse_substitute_range(
        command: &str,
        cursor_row: usize,
        line_count: usize,
    ) -> Option<(usize, usize)> {
        let (_, prefix, _) = Self::substitute_parts(command.trim_start_matches(':'))?;
        let last_row = line_count.saturating_sub(1);

        if prefix.is_empty() {
            let row = cursor_row.min(last_row);
            return Some((row, row));
        }
        if prefix == "%" {
            return Some((0, last_row));
        }

        let (start, end) = prefix.split_once(',').unwrap_or((prefix, prefix));
        let start = start.parse::<usize>().ok()?.checked_sub(1)?;
        let end = end.parse::<usize>().ok()?.checked_sub(1)?;
        (start <= end && end <= last_row).then_some((start, end))
    }

    fn overlay_search_match(
        line: &mut crate::style::StyledLine,
        byte_range: std::ops::Range<usize>,
    ) {
        let byte_start = byte_range.start.min(line.text.len());
        let byte_end = byte_range.end.min(line.text.len());
        if byte_start >= byte_end
            || !line.text.is_char_boundary(byte_start)
            || !line.text.is_char_boundary(byte_end)
        {
            return;
        }

        let start_col = line.text[..byte_start].chars().count();
        let end_col = line.text[..byte_end].chars().count();
        let mut spans = Vec::with_capacity(line.spans.len() + 1);
        for span in line.spans.drain(..) {
            if span.end_col <= start_col || span.start_col >= end_col {
                spans.push(span);
                continue;
            }
            if span.start_col < start_col {
                spans.push(crate::style::Span {
                    start_col: span.start_col,
                    end_col: start_col,
                    style: span.style,
                });
            }
            if span.end_col > end_col {
                spans.push(crate::style::Span {
                    start_col: end_col,
                    end_col: span.end_col,
                    style: span.style,
                });
            }
        }
        spans.push(crate::style::Span {
            start_col,
            end_col,
            style: crate::style::SemanticStyle::Match,
        });
        spans.sort_by_key(|span| span.start_col);
        line.spans = spans;
    }
}
