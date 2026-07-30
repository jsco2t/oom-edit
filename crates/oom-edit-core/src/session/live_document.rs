//! Canonical live document state.
//!
//! This child module is the only session component that owns mutable editor
//! text and its synchronously-derived highlighting/front-matter caches.

use std::ops::Range;

use crate::frontmatter::{parse_front_matter, FrontMatter};
use crate::input::KeyInput;
use crate::spell::{Diagnostic, SpellState};
use crate::syntax::Highlighter;
use crate::vim::{
    Mode as VimMode, ProjectedSelection, RangeOperator, Register, UndoMark, VimCore, VimEffect,
};

/// Result of one atomic live-document mutation.
#[must_use = "session mutations must consume their effects and invalidate rendered state"]
pub(super) struct MutationOutcome {
    pub(super) effects: Vec<VimEffect>,
}

impl IntoIterator for MutationOutcome {
    type Item = VimEffect;
    type IntoIter = std::vec::IntoIter<VimEffect>;

    fn into_iter(self) -> Self::IntoIter {
        self.effects.into_iter()
    }
}

/// Sole owner of mutable live text and caches derived from that text.
pub(super) struct LiveDocument {
    vim: VimCore,
    highlighter: Highlighter,
    front_matter: FrontMatter,
    spell: SpellState,
}

impl LiveDocument {
    pub(super) fn new(text: &str) -> Self {
        Self {
            vim: VimCore::new(text),
            highlighter: Highlighter::new(text),
            front_matter: parse_front_matter(text),
            spell: SpellState::new(text.len()),
        }
    }

    pub(super) fn text(&self) -> String {
        self.vim.text()
    }

    pub(super) fn mode(&self) -> VimMode {
        self.vim.mode()
    }

    pub(super) fn cursor(&self) -> (usize, usize) {
        self.vim.cursor()
    }

    pub(super) fn cursor_byte_offset(&self) -> usize {
        self.vim.cursor_byte_offset()
    }

    pub(super) fn position_for_byte_offset(&self, offset: usize) -> (usize, usize) {
        self.vim.position_for_byte_offset(offset)
    }

    pub(super) fn byte_before_is_newline(&self, offset: usize) -> bool {
        self.vim.byte_before_is_newline(offset)
    }

    pub(super) fn line_count(&self) -> usize {
        self.vim.line_count()
    }

    pub(super) fn line(&self, index: usize) -> Option<String> {
        self.vim.line(index)
    }

    pub(super) fn highlighter(&self) -> &Highlighter {
        &self.highlighter
    }

    pub(super) fn text_ref(&self) -> &str {
        self.highlighter.text()
    }

    pub(super) fn spell_enabled(&self) -> bool {
        self.spell.enabled()
    }

    pub(super) fn set_spell_enabled(&mut self, enabled: bool) {
        self.spell.set_enabled(enabled);
    }

    pub(super) fn diagnostics(&self) -> &[Diagnostic] {
        self.spell.diagnostics()
    }

    pub(super) fn diagnostics_pending(&self) -> bool {
        self.spell.pending()
    }

    pub(super) fn spell_tick(&mut self, engine: &oom_spell::SpellEngine, max_bytes: usize) -> bool {
        self.spell.tick(
            self.highlighter.text(),
            &self.highlighter,
            engine,
            max_bytes,
        )
    }

    pub(super) fn front_matter(&self) -> &FrontMatter {
        &self.front_matter
    }

    pub(super) fn save_point(&mut self) -> UndoMark {
        self.vim.save_point()
    }

    pub(super) fn is_modified_since(&self, mark: UndoMark) -> bool {
        self.vim.is_modified_since(mark)
    }

    pub(super) fn set_viewport(&mut self, top_line: usize, height: u16) {
        self.vim.set_viewport(top_line, height);
    }

    pub(super) fn search_matches_for_line(&mut self, line: usize) -> Vec<Range<usize>> {
        self.vim.search_matches_for_line(line)
    }

    pub(super) fn jump_to(&mut self, row: usize, col: usize) {
        self.vim.jump_to(row, col);
    }

    pub(super) fn clear_search_highlight(&mut self) {
        self.vim.clear_search_highlight();
    }

    pub(super) fn handle_key(&mut self, key: KeyInput) -> MutationOutcome {
        let effects = self.vim.handle_key(key);
        self.refresh_derived(&effects);
        MutationOutcome { effects }
    }

    pub(super) fn has_pending_input(&self) -> bool {
        self.vim.has_pending_input()
    }

    pub(super) fn insert_text(&mut self, text: &str) -> MutationOutcome {
        let edits = self.vim.insert_text(text);
        let effects = if edits.is_empty() {
            Vec::new()
        } else {
            vec![VimEffect::Edited { edits }]
        };
        self.refresh_derived(&effects);
        MutationOutcome { effects }
    }

    pub(super) fn apply_selection(
        &mut self,
        selection: ProjectedSelection,
        operator: RangeOperator,
        register: Register,
    ) -> MutationOutcome {
        let effects = self.vim.apply_selection(selection, operator, register);
        self.refresh_derived(&effects);
        MutationOutcome { effects }
    }

    pub(super) fn substitute(
        &mut self,
        command: &str,
        start_row: usize,
        end_row: usize,
    ) -> Result<MutationOutcome, String> {
        let edits = self.vim.substitute(command, start_row, end_row)?;
        let effects = if edits.is_empty() {
            Vec::new()
        } else {
            vec![VimEffect::Edited { edits }]
        };
        self.refresh_derived(&effects);
        Ok(MutationOutcome { effects })
    }

    pub(super) fn replace_range(
        &mut self,
        range: Range<usize>,
        replacement: &str,
    ) -> Option<MutationOutcome> {
        let edits = self.vim.replace_range(range, replacement)?;
        let effects = if edits.is_empty() {
            Vec::new()
        } else {
            vec![VimEffect::Edited { edits }]
        };
        self.refresh_derived(&effects);
        Some(MutationOutcome { effects })
    }

    fn refresh_derived(&mut self, effects: &[VimEffect]) {
        let mut edited = false;
        for effect in effects {
            if let VimEffect::Edited { edits } = effect {
                let invalidation = self
                    .spell
                    .prepare_invalidation(self.highlighter.text(), edits);
                self.highlighter.apply_edit(edits);
                self.spell.invalidate(invalidation, self.highlighter.text());
                edited = true;
            }
        }
        if edited {
            self.front_matter = parse_front_matter(&self.vim.text());
        }
    }

    #[cfg(test)]
    pub(super) fn reset_work_counters(&self) {
        self.vim.reset_work_counters();
    }

    #[cfg(test)]
    pub(super) fn work_counters(&self) -> (usize, usize) {
        self.vim.work_counters()
    }
}
