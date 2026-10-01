//! Canonical live document state.
//!
//! This child module is the only session component that owns mutable editor
//! text and its synchronously-derived highlighting/front-matter caches.

use std::ops::Range;

use crate::frontmatter::{front_matter_span, parse_front_matter, FrontMatter};
use crate::input::KeyInput;
use crate::rendered::{BlockModel, ModelChange, RetainedBlockModel};
use crate::spell::{Diagnostic, SpellState};
use crate::syntax::Highlighter;
use crate::vim::{
    collapsed_descending_edit, Mode as VimMode, ProjectedSelection, ProjectedYank, RangeOperator,
    Register, UndoMark, VimCore, VimEffect,
};

/// Result of one atomic live-document mutation.
#[must_use = "session mutations must consume their effects and invalidate rendered state"]
pub(super) struct MutationOutcome {
    pub(super) effects: Vec<VimEffect>,
}

/// Batched projection changes are conservatively widened until row updates
/// can be published together from the final authoritative text.
pub(super) enum PendingProjectionChange {
    None,
    One(ModelChange),
    Multiple,
}

impl PendingProjectionChange {
    fn push(&mut self, change: ModelChange) {
        *self = match std::mem::replace(self, Self::None) {
            Self::None => Self::One(change),
            Self::One(_) | Self::Multiple => Self::Multiple,
        };
    }
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
    rendered_model: Option<RetainedBlockModel>,
    pending_projection_change: PendingProjectionChange,
    #[cfg(test)]
    front_matter_reparses: usize,
    #[cfg(test)]
    last_selection_profile_ns: [u128; 4],
}

impl LiveDocument {
    pub(super) fn new(text: &str) -> Self {
        Self {
            vim: VimCore::new(text),
            highlighter: Highlighter::new(text),
            front_matter: parse_front_matter(text),
            spell: SpellState::new(text.len()),
            rendered_model: None,
            pending_projection_change: PendingProjectionChange::None,
            #[cfg(test)]
            front_matter_reparses: 0,
            #[cfg(test)]
            last_selection_profile_ns: [0; 4],
        }
    }

    /// Atomically replace authoritative text and every derived cache.
    pub(super) fn reload(&mut self, text: &str, cursor: (usize, usize)) -> MutationOutcome {
        let spell_enabled = self.spell.enabled();
        // The old rendered model cannot describe the replacement text. Release
        // it before building new source caches to avoid holding both models.
        self.rendered_model = None;
        let mut replacement = Self::new(text);
        replacement.spell.set_enabled(spell_enabled);
        let row = cursor.0.min(replacement.line_count().saturating_sub(1));
        replacement.jump_to(row, cursor.1);
        *self = replacement;
        MutationOutcome {
            effects: Vec::new(),
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

    pub(super) fn ensure_rendered_model(&mut self) {
        if self.rendered_model.is_none() {
            self.rendered_model = Some(RetainedBlockModel::new(self.highlighter.text()));
        }
    }

    pub(super) fn rendered_model(&self) -> &BlockModel {
        self.rendered_model
            .as_ref()
            .expect("rendered model must be prepared before use")
            .model()
    }

    pub(super) fn take_projection_change(&mut self) -> PendingProjectionChange {
        std::mem::replace(
            &mut self.pending_projection_change,
            PendingProjectionChange::None,
        )
    }

    #[cfg(test)]
    pub(super) fn rendered_model_work(&self) -> crate::rendered::ModelWork {
        self.rendered_model
            .as_ref()
            .expect("rendered model must be prepared before work inspection")
            .work()
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

    pub(super) fn clear_pending_input(&mut self) {
        self.vim.clear_pending_input();
    }

    pub(super) fn set_host_time(&mut self, now: Option<std::time::Duration>) {
        self.vim.set_host_time(now);
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
        #[cfg(test)]
        {
            self.last_selection_profile_ns = [0; 4];
        }
        #[cfg(test)]
        let started = std::time::Instant::now();
        let effects = self.vim.apply_selection(selection, operator, register);
        #[cfg(test)]
        {
            self.last_selection_profile_ns[0] = started.elapsed().as_nanos();
        }
        self.refresh_derived(&effects);
        MutationOutcome { effects }
    }

    pub(super) fn apply_yank(
        &mut self,
        yank: ProjectedYank,
        register: Register,
    ) -> MutationOutcome {
        let effects = self.vim.apply_yank(yank, register);
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
        let mut front_matter_affected = false;
        for effect in effects {
            if let VimEffect::Edited { edits } = effect {
                let invalidation = self
                    .spell
                    .prepare_invalidation(self.highlighter.text(), edits);
                if let Some(collapsed) = collapsed_descending_edit(self.highlighter.text(), edits) {
                    front_matter_affected |=
                        edit_may_change_front_matter(self.highlighter.text(), &collapsed);
                    let local_window = self.highlighter.local_section_window(&collapsed);
                    #[cfg(test)]
                    let started = std::time::Instant::now();
                    let scope = self.rendered_model.as_ref().map(|model| {
                        let ordinary = model.prepare_edit(self.highlighter.text(), &collapsed);
                        if matches!(ordinary, crate::rendered::ModelEditScope::Wide) {
                            local_window
                                .and_then(|window| {
                                    model.prepare_window_edit(
                                        self.highlighter.text(),
                                        &collapsed,
                                        window,
                                    )
                                })
                                .unwrap_or(ordinary)
                        } else {
                            ordinary
                        }
                    });
                    #[cfg(test)]
                    {
                        self.last_selection_profile_ns[1] += started.elapsed().as_nanos();
                    }
                    #[cfg(test)]
                    let started = std::time::Instant::now();
                    self.highlighter.apply_edit(edits);
                    #[cfg(test)]
                    {
                        self.last_selection_profile_ns[2] += started.elapsed().as_nanos();
                    }
                    if let (Some(model), Some(scope)) = (&mut self.rendered_model, scope) {
                        #[cfg(test)]
                        let started = std::time::Instant::now();
                        let change = model.apply_edit(self.highlighter.text(), &collapsed, scope);
                        #[cfg(test)]
                        {
                            self.last_selection_profile_ns[3] += started.elapsed().as_nanos();
                        }
                        self.pending_projection_change.push(change);
                    }
                    self.spell.invalidate(invalidation, self.highlighter.text());
                    continue;
                }
                for edit in edits {
                    front_matter_affected |=
                        edit_may_change_front_matter(self.highlighter.text(), edit);
                    #[cfg(test)]
                    let started = std::time::Instant::now();
                    let scope = self
                        .rendered_model
                        .as_ref()
                        .map(|model| model.prepare_edit(self.highlighter.text(), edit));
                    #[cfg(test)]
                    {
                        self.last_selection_profile_ns[1] += started.elapsed().as_nanos();
                    }
                    #[cfg(test)]
                    let started = std::time::Instant::now();
                    self.highlighter.apply_edit(std::slice::from_ref(edit));
                    #[cfg(test)]
                    {
                        self.last_selection_profile_ns[2] += started.elapsed().as_nanos();
                    }
                    if let (Some(model), Some(scope)) = (&mut self.rendered_model, scope) {
                        #[cfg(test)]
                        let started = std::time::Instant::now();
                        let change = model.apply_edit(self.highlighter.text(), edit, scope);
                        #[cfg(test)]
                        {
                            self.last_selection_profile_ns[3] += started.elapsed().as_nanos();
                        }
                        self.pending_projection_change.push(change);
                    }
                }
                self.spell.invalidate(invalidation, self.highlighter.text());
            }
        }
        if front_matter_affected {
            self.front_matter = parse_front_matter(self.highlighter.text());
            #[cfg(test)]
            {
                self.front_matter_reparses += 1;
            }
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

    #[cfg(test)]
    pub(super) fn selection_profile_ns(&self) -> [u128; 4] {
        self.last_selection_profile_ns
    }
}

fn edit_may_change_front_matter(text: &str, edit: &crate::vim::TextEdit) -> bool {
    if let Some(span) = front_matter_span(text) {
        return edit.range.start <= span.end;
    }
    let first_line_end = text.find('\n').map_or(text.len(), |at| at + 1);
    edit.range.start <= first_line_end
}

#[cfg(test)]
mod tests {
    use super::{LiveDocument, PendingProjectionChange};
    use crate::frontmatter::parse_front_matter;
    use crate::rendered::{BlockModel, ModelChange};
    use crate::syntax::Highlighter;

    #[test]
    fn row_projection_change_is_single_use_and_batches_conservatively() {
        let mut live = LiveDocument::new("# Heading\n\nA body paragraph.\n\nTail.\n");
        live.ensure_rendered_model();
        live.jump_to(2, 2);
        let _ = live.insert_text("fresh ");
        assert!(matches!(
            live.take_projection_change(),
            PendingProjectionChange::One(ModelChange::Local { index: 1, .. })
        ));
        assert!(matches!(
            live.take_projection_change(),
            PendingProjectionChange::None
        ));
        let _ = live.insert_text("first ");
        let _ = live.insert_text("second ");
        assert!(matches!(
            live.take_projection_change(),
            PendingProjectionChange::Multiple
        ));
        let _ = live.reload("Replacement.\n", (0, 0));
        assert!(matches!(
            live.take_projection_change(),
            PendingProjectionChange::None
        ));
    }

    #[test]
    fn reload_replaces_the_retained_model_and_global_indices() {
        let mut live = LiveDocument::new("[ref]: /old\n\nA [link][ref].\n");
        live.ensure_rendered_model();
        let replacement = "---\ntitle: New\n---\n\n# Heading\n\n[^note]: body\n\nA [^note].\n";
        let _outcome = live.reload(replacement, (0, 0));
        live.ensure_rendered_model();
        assert_eq!(
            live.rendered_model(),
            &BlockModel::build(
                replacement,
                crate::frontmatter::front_matter_span(replacement)
            )
        );
        assert!(live.rendered_model_work().full_rebuild);
    }

    #[test]
    fn reload_replaces_source_styling_after_fence_language_change() {
        let original = "```rust\nfn value() -> usize { 12 }\n```\n";
        let replacement = "```go\nfunc value() int { return 12 }\n```\n";
        let mut live = LiveDocument::new(original);
        let _ = live
            .highlighter
            .highlight_lines(0..original.lines().count());
        let _ = live.reload(replacement, (1, 0));
        let fresh = Highlighter::new(replacement);
        assert_eq!(live.highlighter.text(), replacement);
        assert_eq!(
            live.highlighter
                .highlight_lines(0..replacement.lines().count()),
            fresh.highlight_lines(0..replacement.lines().count()),
        );
    }

    #[test]
    fn mutation_gateway_publishes_current_model_before_returning() {
        let mut live = LiveDocument::new("# Heading\n\nA body paragraph.\n\nTail.\n");
        live.ensure_rendered_model();
        live.jump_to(2, 6);
        let _outcome = live.insert_text("fresh ");
        let current = live.text();
        assert_eq!(
            live.rendered_model(),
            &BlockModel::build(&current, crate::frontmatter::front_matter_span(&current))
        );
        assert_eq!(live.rendered_model_work().rebuilt_blocks, 1);
    }

    #[test]
    fn distant_code_edits_retain_unchanged_front_matter_analysis() {
        let text = "---\ntitle: Note\n---\n\n```rust\nfn main() {}\n```\n";
        let mut live = LiveDocument::new(text);
        let initial = parse_front_matter(text);
        live.jump_to(5, 3);
        let _ = live.insert_text("fresh ");
        assert_eq!(live.front_matter(), &initial);
        assert_eq!(live.front_matter_reparses, 0);
        live.jump_to(1, 7);
        let _ = live.insert_text("New ");
        assert_ne!(live.front_matter(), &initial);
        assert_eq!(live.front_matter_reparses, 1);
    }

    #[test]
    fn paste_refresh_borrows_updated_text() {
        let many_lines = format!("{}Cafe body\n", "λ preceding\r\n".repeat(10_000));
        for (initial, row, column) in [
            ("---\ntitle: \"Cafe\"\n---\n\nBody\n", 1, 8),
            ("---\r\ntitle: \"Cafe\"\r\n---\r\n\r\nBody\r\n", 1, 8),
            ("+++\ntitle = \"Cafe\"\n+++\n\nBody\n", 1, 9),
            ("---\ntitle: [Cafe\n---\n\nBody\n", 1, 8),
            ("Cafe body\n", 0, 0),
            (many_lines.as_str(), 10_000, 0),
        ] {
            let expected = initial.replacen("Cafe", "éCafe", 1);
            let mut live = LiveDocument::new(initial);
            live.jump_to(row, column);
            live.reset_work_counters();
            let outcome = live.insert_text("é");
            assert_eq!(outcome.effects.len(), 1);
            assert_eq!(live.work_counters(), (0, 0));
            assert_eq!(live.text_ref(), expected);
            assert_eq!(live.front_matter(), &parse_front_matter(&expected));
            assert_eq!(live.cursor(), (row, column + 1));
            assert_eq!(live.text(), expected);
        }
    }
}
