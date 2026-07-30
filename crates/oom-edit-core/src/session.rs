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

    #[test]
    fn session_starts_in_rendered_normal() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        assert_eq!(session.mode(), Mode::Normal);
        assert_eq!(session.cursor(), (0, 0));
        assert!(!session.render_layout(31).lines.is_empty());
        assert_eq!(session.rendered_cursor_line(), 0);
    }

    #[test]
    fn rendered_navigation_updates_canonical_source_cursor() {
        let mut session = EditorSession::from_text("# Heading\n\nBody\n");
        session.render_layout(40);
        session.handle_key(key('j'));
        assert_eq!(session.cursor(), (0, 0));
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
        assert_eq!(session.live.work_counters(), (2, 1));
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
        assert_eq!(selection.source_ranges, vec![0..10]);
        assert_eq!(&text[selection.source_ranges[0].clone()], "# Heading\n");
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
    /// Yanked text to the system clipboard (e.g. `"+y`).
    ClipboardWrite(String),
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
    ProjectedBlockRow, ProjectedSelection, RangeOperator, Register, UndoMark, VimEffect,
};

// ── Document (internal) ───────────────────────────────────────────────────

use crate::document::{Document, LineEnding};
use crate::error::{OpenError, SaveError};
use crate::frontmatter::FrontMatter;
use crate::rendered::nav;
use crate::rendered::BlockModel;
use crate::spell::{
    DecorationKind, Diagnostic, DiagnosticDecorationRow, DiagnosticProvider, PositionError,
    TextPosition,
};
use crate::style::{
    RenderedCursor, RenderedLayout, RenderedPoint, RenderedSearch, RenderedSelection,
    RenderedSourceAtom, SearchDirection, SelectionShape, SourceDecoration,
};
use live_document::LiveDocument;
use std::ops::Range;
use unicode_width::UnicodeWidthChar;

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
                    selected_width: row.columns.end.saturating_sub(row.columns.start),
                    ranges: row.source_ranges,
                })
                .collect(),
        },
    }
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
    Command,
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
struct RenderedState {
    /// Cached rendered layout (None = needs rebuild).
    layout_cache: Option<RenderedLayout>,
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
}

impl RenderedState {
    fn new() -> Self {
        Self {
            layout_cache: None,
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
        }
    }

    fn needs_layout(&self, width: u16) -> bool {
        self.layout_cache.is_none() || self.last_width != width
    }

    /// Invalidate the layout cache.
    fn invalidate(&mut self) {
        self.layout_cache = None;
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
    save_point: UndoMark,
    /// Buffer for ex-command text in Command mode.
    command_buffer: String,
    /// The document model — text, path, front matter, I/O state.
    document: Document,
    /// Persistent rendered navigation and Select state.
    rendered_state: RenderedState,
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
            save_point,
            command_buffer: String::new(),
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
        let mut live = LiveDocument::new(&text);
        let save_point = live.save_point();
        Ok(Self {
            live,
            session_mode: SessionMode::CoreDriven,
            save_point,
            command_buffer: String::new(),
            document,
            rendered_state: RenderedState::new(),
        })
    }

    /// Save the document to its path (or the given override path).
    ///
    /// `force: true` bypasses external-modification detection (FR-5.7).
    ///
    /// Returns `SaveError::ExternallyModified` if the file was externally
    /// modified and `force` is `false` (FR-5.7).
    pub fn save(&mut self, path: Option<&std::path::Path>, force: bool) -> Result<(), SaveError> {
        // Get the current text from the vim buffer
        let text = self.live.text();
        // Save using the document's I/O logic, passing the vim buffer text
        self.document.save_with_text(&text, path, force)?;
        // The vim engine owns the authoritative dirty generation. Capture it
        // once after the save succeeds.
        let mark = self.live.save_point();
        self.save_point = mark;
        self.rendered_state.invalidate();
        Ok(())
    }

    /// Atomically save a copy without retargeting the buffer or clearing its
    /// dirty state (`:w {path}`).
    pub fn save_copy(&self, path: &std::path::Path) -> Result<(), SaveError> {
        self.document.save_copy_with_text(&self.live.text(), path)
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
            SessionMode::Command => Mode::Command,
        }
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
    /// Call [`Self::render_layout`] first to establish the host width. Rows
    /// outside `visible` are clipped, and source-less presentation atoms are
    /// never included.
    pub fn diagnostic_decoration_rows(
        &mut self,
        visible: Range<usize>,
    ) -> Vec<DiagnosticDecorationRow> {
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return Vec::new();
        };
        let start = visible.start.min(layout.lines.len());
        let end = visible.end.min(layout.lines.len());
        if start >= end {
            return Vec::new();
        }
        let source_intervals = Self::visible_source_intervals(
            layout.lines[start..end]
                .iter()
                .map(|line| line.atoms.as_slice()),
        );
        let diagnostics = self.diagnostics();
        Self::visible_diagnostic_indices(diagnostics, &source_intervals)
            .into_iter()
            .flat_map(|index| {
                let diagnostic = &diagnostics[index];
                let kind = DecorationKind::Diagnostic {
                    provider: diagnostic.provider,
                    severity: diagnostic.severity,
                };
                nav::project_source_range(&diagnostic.range, visible.clone(), layout)
                    .into_iter()
                    .map(move |(row, columns)| DiagnosticDecorationRow { row, columns, kind })
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

    /// Return cursor position as `(line, col)` — 0-based.
    pub fn cursor(&self) -> (usize, usize) {
        self.live.cursor()
    }

    /// Return the unprefixed command-line text, or `None` outside Command mode.
    pub fn command_line(&self) -> Option<String> {
        (self.mode() == Mode::Command).then(|| self.command_buffer.clone())
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
        let layout = self.rendered_state.layout_cache.as_ref()?;
        let mut selection = nav::project_selection_from_source_positions(
            active.anchor.point,
            active.active.point,
            active.kind.shape(),
            active.anchor.source,
            active.active.source,
            layout,
            &self.live.text(),
        );
        if let SelectionKind::Character { ranges } = &active.kind {
            selection.source_ranges = ranges.clone();
        }
        Some(selection)
    }

    /// Check if the buffer is dirty (modified since last save).
    pub fn is_dirty(&self) -> bool {
        self.live.is_modified_since(self.save_point)
    }

    /// Take a save point (marks current state as clean).
    pub fn save_point(&mut self) {
        self.save_point = self.live.save_point();
    }

    /// Insert text at the current cursor position as a single paste operation.
    ///
    /// This is used for bracketed paste (FR-5.5): the text is inserted as one
    /// undo step with no per-character processing. The text is always inserted
    /// in Insert mode — if not in Insert mode, no action is taken.
    ///
    /// Returns `Effect::Edited` if text was inserted, `Effect::Message` if
    /// ignored (not in Insert mode).
    pub fn insert_paste(&mut self, text: &str) -> Vec<Effect> {
        // Only paste in Insert mode (FR-5.5)
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
            text: line.clone(),
            spans: Vec::new(),
        };
        let mut wrapped = crate::rendered::wrap_source_line(&styled, width);
        if (doc_line, doc_col) == self.cursor()
            && self.mode() == Mode::Insert
            && Self::cursor_needs_blank_continuation(&line, &wrapped, doc_col, width)
        {
            wrapped.push(crate::style::StyledLine {
                text: String::new(),
                spans: Vec::new(),
            });
        }
        let (row, _) = Self::wrapped_cursor_position(&line, &wrapped, doc_col);
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
        let mut screen_cursor = (0usize, 0usize);
        let mut line_start = Self::source_line_start(self.live.text_ref(), start_line);

        if vp.wrap {
            for (offset, styled_line) in highlighted.iter().enumerate() {
                let doc_line = start_line + offset;
                let mut wrapped = crate::rendered::wrap_source_line(styled_line, vp.width);
                if doc_line == cursor_line
                    && self.mode() == Mode::Insert
                    && Self::cursor_needs_blank_continuation(
                        &styled_line.text,
                        &wrapped,
                        cursor_col,
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
                    let (wrapped_row, wrapped_col) =
                        Self::wrapped_cursor_position(&styled_line.text, &wrapped, cursor_col);
                    screen_cursor = (
                        first_screen_row + wrapped_row.saturating_sub(skip),
                        wrapped_col,
                    );
                }

                let mut wrapped_source_start = line_start;
                for (wrapped_row, row) in wrapped.into_iter().enumerate() {
                    let atoms = Self::source_atoms(&row.text, wrapped_source_start);
                    wrapped_source_start = wrapped_source_start.saturating_add(row.text.len());
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
                if doc_line == cursor_line {
                    screen_cursor = (
                        lines.len(),
                        cursor_col
                            .saturating_sub(vp.left_col)
                            .min(vp.width.saturating_sub(1) as usize),
                    );
                }
                source_rows.push(Self::horizontal_window_atoms(
                    &styled_line.text,
                    line_start,
                    vp.left_col,
                    vp.width,
                ));
                lines.push(Self::horizontal_window(styled_line, vp.left_col, vp.width));
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
        }

        // Truncate to exactly viewport.height (in case we over-highlighted)
        lines.truncate(vp.height as usize);
        line_numbers.truncate(vp.height as usize);
        source_rows.truncate(vp.height as usize);

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

        crate::style::SourceFrame {
            lines,
            decorations,
            line_numbers,
            first_line_number: first_visible + 1,
            cursor: (
                screen_cursor.0.min(vp.height.saturating_sub(1) as usize) as u16,
                screen_cursor.1.min(vp.width.saturating_sub(1) as usize) as u16,
            ),
        }
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

    fn source_atoms(text: &str, source_start: usize) -> Vec<RenderedSourceAtom> {
        let mut atoms: Vec<RenderedSourceAtom> = Vec::new();
        let mut column = 0usize;
        for (byte, character) in text.char_indices() {
            let source = source_start + byte..source_start + byte + character.len_utf8();
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

    fn horizontal_window_atoms(
        text: &str,
        source_start: usize,
        left_col: usize,
        width: u16,
    ) -> Vec<RenderedSourceAtom> {
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let width = usize::from(width);
        if width == 0 || left_col >= chars.len() {
            return Vec::new();
        }
        let end_col = left_col.saturating_add(width).min(chars.len());
        let right_clipped = chars.len() > end_col;
        let mut atoms: Vec<RenderedSourceAtom> = Vec::new();
        let mut display_column = 0usize;
        for (window_index, &(byte, character)) in chars[left_col..end_col].iter().enumerate() {
            let synthetic = (left_col > 0 && window_index == 0)
                || (right_clipped && window_index + 1 == end_col - left_col);
            let display_width = if synthetic {
                1
            } else {
                character.width().unwrap_or(0)
            };
            let source = (!synthetic)
                .then_some(source_start + byte..source_start + byte + character.len_utf8());
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

        if left_col > 0 {
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
                SessionMode::CoreDriven | SessionMode::Command => None,
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
            let text = self.live.text();
            let fm_span = crate::frontmatter::front_matter_span(&text);
            let model = BlockModel::build(&text, fm_span);
            let layout = RenderedLayout::build_with_front_matter_state(
                &model,
                width,
                self.live.highlighter(),
                self.rendered_state.fm_collapsed,
            );
            let cursor = active_atom_remap
                .then(|| {
                    selection
                        .as_ref()
                        .and_then(|selection| selection.active.atom.as_ref())
                        .and_then(|source| nav::point_for_source_range(source, &layout))
                })
                .flatten()
                .or_else(|| {
                    (block_selection && !active_atom_remap)
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
                    nav::enter_rendered(source_anchor.0, source_anchor.1, &layout, &text)
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
                    (block_selection && !anchor_atom_remap)
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
                        nav::enter_rendered(line, col, &layout, &text).point()
                    })
                });
            self.rendered_state.layout_cache = Some(layout);
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
            }
        }
        self.rendered_state
            .layout_cache
            .as_ref()
            .expect("rendered layout must be cached after building")
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
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let text = self.live.text();
        self.rendered_state.cursor = nav::enter_rendered(edit_line, edit_col, layout, &text);
    }

    // ── Internal helpers ─────────────────────────────────────────────

    /// Handle keys in Command mode (ex-command entry).
    fn handle_command_mode_key(&mut self, key: KeyInput) -> Vec<Effect> {
        let mut effects = Vec::new();
        match key.code.kind {
            KeyCodeKind::Esc => {
                // Cancel command-line and return to Normal
                self.command_buffer.clear();
                self.session_mode = SessionMode::CoreDriven;
                effects.push(Effect::ModeChanged(Mode::Normal));
            }
            KeyCodeKind::Enter => {
                // Execute the command from the buffer
                let cmd = self.command_buffer.trim().to_string();
                self.command_buffer.clear();
                effects.extend(self.process_ex_command(&cmd));
                // Only default to Normal if the ex command didn't already change mode
                if !effects.iter().any(|e| matches!(e, Effect::ModeChanged(_))) {
                    self.session_mode = SessionMode::CoreDriven;
                    effects.push(Effect::ModeChanged(Mode::Normal));
                }
            }
            KeyCodeKind::Backspace => {
                // Remove last character from command buffer
                self.command_buffer.pop();
            }
            _ => {
                // Collect printable characters in the command buffer
                if let KeyCodeKind::Char(c) = key.code.kind {
                    if !key.mods.ctrl && !key.mods.alt && !key.mods.shift {
                        self.command_buffer.push(c);
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
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('v' | 'V')) {
            return self.enter_select(SelectionShape::Block);
        }
        if key.mods == Modifiers::default() {
            match key.code.kind {
                KeyCodeKind::Char('v') => {
                    return self.enter_select(SelectionShape::Character);
                }
                KeyCodeKind::Char('V') => {
                    return self.enter_select(SelectionShape::Line);
                }
                KeyCodeKind::Char('"') => {
                    self.rendered_state.register_input = RegisterInput::AwaitingName;
                    return Vec::new();
                }
                KeyCodeKind::Char(':') => {
                    self.command_buffer.clear();
                    self.rendered_state.search.cancel();
                    self.session_mode = SessionMode::Command;
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
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('c')) {
            return self.finish_select(Mode::Normal, Vec::new());
        }
        if key.mods.ctrl && matches!(key.code.kind, KeyCodeKind::Char('v' | 'V')) {
            return self.switch_or_cancel_selection_shape(SelectionShape::Block);
        }
        if key.mods == Modifiers::default() {
            match key.code.kind {
                KeyCodeKind::Esc => return self.finish_select(Mode::Normal, Vec::new()),
                KeyCodeKind::Char('v') => {
                    return self.switch_or_cancel_selection_shape(SelectionShape::Character)
                }
                KeyCodeKind::Char('V') => {
                    return self.switch_or_cancel_selection_shape(SelectionShape::Line)
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
                KeyCodeKind::Char('y') => return self.apply_select_operator(RangeOperator::Yank),
                KeyCodeKind::Char('d') | KeyCodeKind::Char('x') => {
                    return self.apply_select_operator(RangeOperator::Delete)
                }
                KeyCodeKind::Char('c') => return self.apply_select_operator(RangeOperator::Change),
                KeyCodeKind::Char('>') => return self.apply_select_operator(RangeOperator::Indent),
                KeyCodeKind::Char('<') => {
                    return self.apply_select_operator(RangeOperator::Outdent)
                }
                _ => {
                    self.rendered_state.register_input = RegisterInput::Default;
                }
            }
        }
        self.handle_rendered_navigation_key(key)
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

        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return Vec::new();
        };
        let text = nav::key_inspects_source(key).then(|| self.live.text());
        let cursor = self.rendered_state.cursor;
        let search = self.rendered_state.search.current().cloned();
        let count = std::mem::take(&mut self.rendered_state.count);
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
                let text = self.live.text();
                let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
                    return Vec::new();
                };
                let Some((search_state, _)) = self.rendered_state.search.prompt() else {
                    return Vec::new();
                };
                let mut search_state = search_state.clone();
                search_state.pattern.push(c);
                let match_line = nav::find_next_match(
                    &search_state,
                    &cursor,
                    layout,
                    &text,
                    search_state.direction(),
                );
                self.rendered_state
                    .search
                    .update_draft(|draft| *draft = search_state);
                if let Some(match_line) = match_line {
                    self.rendered_state.cursor = nav::cursor_for_row(
                        match_line,
                        self.rendered_state.cursor.desired_column,
                        layout,
                    );
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
        self.rendered_state.search.cancel();
        self.rendered_state.count = 0;
        let vim_effects = self.live.handle_key(action.key());
        self.translate_vim_effects(vim_effects)
    }

    fn enter_select(&mut self, shape: SelectionShape) -> Vec<Effect> {
        let point = self.rendered_state.cursor.point();
        let source = self.live.cursor();
        let atom = self
            .rendered_state
            .layout_cache
            .as_ref()
            .and_then(|layout| nav::source_for_point(point, layout));
        let line = self
            .rendered_state
            .layout_cache
            .as_ref()
            .and_then(|layout| nav::line_identity_for_point(point, layout));
        let endpoint = SelectionEndpoint {
            point,
            source,
            atom,
            line,
        };
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
            SessionMode::CoreDriven | SessionMode::Command => return Vec::new(),
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

    fn apply_select_operator(&mut self, operator: RangeOperator) -> Vec<Effect> {
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return Vec::new();
        };
        let SessionMode::Select(active) = &self.session_mode else {
            return Vec::new();
        };
        let mut selection = nav::project_selection_from_source_positions(
            active.anchor.point,
            active.active.point,
            active.kind.shape(),
            active.anchor.source,
            active.active.source,
            layout,
            &self.live.text(),
        );
        if let SelectionKind::Character { ranges } = &active.kind {
            selection.source_ranges = ranges.clone();
        }
        if selection.source_ranges.is_empty() {
            return Vec::new();
        }
        let register = self.rendered_state.register_input.take();
        let projected = project_selection_for_vim(selection);
        let vim_effects = self.live.apply_selection(projected, operator, register);
        let effects = self.translate_vim_effects(vim_effects);
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
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let source = self.live.cursor();
        self.rendered_state.cursor = nav::enter_rendered_at_offset(
            source.0,
            self.live.cursor_byte_offset(),
            layout,
            |offset| self.live.position_for_byte_offset(offset).0,
            |offset| self.live.byte_before_is_newline(offset),
        );
    }

    fn commit_rendered_cursor(&mut self) {
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let source_offset = nav::canonical_source_offset_for_row(
            &self.rendered_state.cursor,
            self.live.cursor_byte_offset(),
            layout,
        );
        let source = self.live.position_for_byte_offset(source_offset);
        self.live.jump_to(source.0, source.1);
    }

    fn refresh_character_selection(&mut self) {
        let SessionMode::Select(selection) = &mut self.session_mode else {
            return;
        };
        let Some(layout) = self.rendered_state.layout_cache.as_ref() else {
            return;
        };
        let point = self.rendered_state.cursor.point();
        selection.active = SelectionEndpoint {
            point,
            source: self.live.cursor(),
            atom: nav::source_for_point(point, layout),
            line: nav::line_identity_for_point(point, layout),
        };
        if let SelectionKind::Character { ranges } = &mut selection.kind {
            *ranges = nav::project_selection(
                selection.anchor.point,
                selection.active.point,
                SelectionShape::Character,
                layout,
                &self.live.text(),
            )
            .source_ranges;
        }
    }

    fn translate_vim_effects(
        &mut self,
        vim_effects: impl IntoIterator<Item = VimEffect>,
    ) -> Vec<Effect> {
        let mut effects = Vec::new();
        let mut left_insert = false;
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
                    self.rendered_state.invalidate();
                    effects.push(Effect::Edited);
                }
                VimEffect::CursorMoved => effects.push(Effect::CursorMoved),
                VimEffect::ExCommand { command } => {
                    effects.extend(self.process_ex_command(&command))
                }
                VimEffect::CommandCancelled => {}
                VimEffect::ClipboardYank(text) => effects.push(Effect::ClipboardWrite(text)),
                VimEffect::SearchWrapped => effects.push(Effect::Message {
                    text: "Search wrapped around buffer".to_string(),
                    severity: Severity::Info,
                }),
                VimEffect::Bell => {}
            }
        }
        if left_insert {
            // Insert-mode motions are owned by the canonical Vim cursor.
            // Re-enter rendered coordinates before the next rendered motion;
            // when an edit invalidated the cache, render_layout performs this
            // same remap from the canonical source anchor after rebuilding.
            self.remap_active_cursor_from_canonical();
        }
        effects
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
            "e" => vec![Effect::OpenRequested {
                path: args.0.map(std::path::PathBuf::from).unwrap_or_default(),
                force: args.1,
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
        let cmd = cmd.trim_start_matches(':');

        // Special case: substitute commands like "s/pat/rep/" or ":%s/pat/rep/g"
        // have no whitespace separator between base and args. Detect and extract base.
        let (base, rest_str) = if cmd.starts_with("s/") || cmd.starts_with("substitute/") {
            if cmd.starts_with("substitute/") {
                // "substitute/pat/rep/" → base="substitute", rest="/pat/rep/"
                ("substitute", Some(&cmd[10..]))
            } else {
                // "s/pat/rep/" → base="s", rest="/pat/rep/"
                ("s", Some(&cmd[1..]))
            }
        } else if cmd.contains("s/") || cmd.contains("substitute/") {
            // Might be a substitute with range prefix like "%s/pat/rep/g" or "1,2s/pat/rep/"
            // Find the 's/' or 'substitute/' after any range prefix
            if let Some(s_pos) = cmd.find("substitute/") {
                ("substitute", Some(&cmd[s_pos + 10..]))
            } else if let Some(s_pos) = cmd.find("s/") {
                ("s", Some(&cmd[s_pos + 1..]))
            } else {
                let mut parts = cmd.splitn(2, char::is_whitespace);
                (parts.next().unwrap_or(cmd), parts.next())
            }
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

        // Check for ! in args (e.g., :w!)
        let (args, force) = if let Some(a) = rest_str {
            if a.trim().ends_with('!') {
                (Some(&a[..a.trim().len() - 1]), true)
            } else {
                (rest_str, force)
            }
        } else {
            (rest_str, force)
        };

        // Extract path argument (first word of rest)
        let path = args.and_then(|a| {
            let a = a.trim();
            if a.is_empty() {
                None
            } else {
                a.split_whitespace().next()
            }
        });

        (base, (path, force))
    }

    /// Resolve a substitute command's optional 1-based line range.
    /// An omitted range targets the cursor line; `%` targets the whole buffer.
    fn parse_substitute_range(
        command: &str,
        cursor_row: usize,
        line_count: usize,
    ) -> Option<(usize, usize)> {
        let command = command.trim_start_matches(':');
        let substitute_pos = if command.starts_with("substitute/") || command.starts_with("s/") {
            0
        } else {
            command.find("substitute/").or_else(|| command.find("s/"))?
        };
        let prefix = command[..substitute_pos].trim();
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
