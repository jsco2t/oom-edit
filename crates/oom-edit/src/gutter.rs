//! Compact App-owned presentation state for diagnostic gutter markers.

use std::collections::BTreeMap;

use oom_edit_core::DiagnosticSeverity;
use ratatui::style::Modifier;

/// One compact, sorted marker row retained by a completed snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GutterTroubleEntry {
    source_line: usize,
    severity: DiagnosticSeverity,
}

/// Immutable completed marker summary consumed by renderers.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct GutterTroubleSnapshot {
    entries: Vec<GutterTroubleEntry>,
}

impl GutterTroubleSnapshot {
    pub(crate) fn severity(&self, source_line: usize) -> Option<DiagnosticSeverity> {
        self.entries
            .binary_search_by_key(&source_line, |entry| entry.source_line)
            .ok()
            .map(|index| self.entries[index].severity)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn heap_capacity_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<GutterTroubleEntry>()
    }

    #[cfg(test)]
    pub(crate) fn testing(items: &[(usize, DiagnosticSeverity)]) -> Self {
        let projected: Vec<_> = items
            .iter()
            .map(|(source_line, severity)| GutterTroubleItem {
                source_line: *source_line,
                severity: *severity,
            })
            .collect();
        let mut build = PendingGutterTroubleBuild::new(0, projected.len());
        build.advance(projected.len(), |index| Some(projected[index]));
        build
            .publish(0)
            .expect("test gutter snapshot projection is complete")
    }
}

/// Minimal projected input accepted by the resumable builder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GutterTroubleItem {
    pub(crate) source_line: usize,
    pub(crate) severity: DiagnosticSeverity,
}

/// Resumable deduplication work tagged with its owning generation.
#[derive(Debug)]
pub(crate) struct PendingGutterTroubleBuild {
    generation: u64,
    item_count: usize,
    next: usize,
    highest_by_line: BTreeMap<usize, DiagnosticSeverity>,
    mapped_items: usize,
}

impl PendingGutterTroubleBuild {
    pub(crate) fn new(generation: u64, item_count: usize) -> Self {
        Self {
            generation,
            item_count,
            next: 0,
            highest_by_line: BTreeMap::new(),
            mapped_items: 0,
        }
    }

    /// Request and map at most `item_budget` rows from the current publication.
    pub(crate) fn advance(
        &mut self,
        item_budget: usize,
        mut project: impl FnMut(usize) -> Option<GutterTroubleItem>,
    ) -> usize {
        let end = self.next.saturating_add(item_budget).min(self.item_count);
        if end == self.next {
            return 0;
        }
        for index in self.next..end {
            if let Some(item) = project(index) {
                self.highest_by_line
                    .entry(item.source_line)
                    .and_modify(|severity| {
                        if severity_priority(item.severity) > severity_priority(*severity) {
                            *severity = item.severity;
                        }
                    })
                    .or_insert(item.severity);
            }
        }
        self.mapped_items += end - self.next;
        let advanced = end - self.next;
        self.next = end;
        advanced
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.next == self.item_count
    }

    /// Publish only a complete result belonging to the current owner generation.
    pub(crate) fn publish(self, current_generation: u64) -> Option<GutterTroubleSnapshot> {
        if !self.is_complete() || self.generation != current_generation {
            return None;
        }
        Some(GutterTroubleSnapshot {
            entries: self
                .highest_by_line
                .into_iter()
                .map(|(source_line, severity)| GutterTroubleEntry {
                    source_line,
                    severity,
                })
                .collect(),
        })
    }

    #[cfg(test)]
    fn mapped_items(&self) -> usize {
        self.mapped_items
    }
}

fn severity_priority(severity: DiagnosticSeverity) -> u8 {
    match severity {
        DiagnosticSeverity::Error => 4,
        DiagnosticSeverity::Warning => 3,
        DiagnosticSeverity::Info => 2,
        DiagnosticSeverity::Hint => 1,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GutterMarkerRole {
    Error,
    Warning,
    Info,
    Muted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GutterMarkerStyle {
    pub(crate) glyph: char,
    pub(crate) role: GutterMarkerRole,
    pub(crate) modifier: Modifier,
}

pub(crate) fn marker_style(severity: DiagnosticSeverity) -> GutterMarkerStyle {
    let (glyph, role) = match severity {
        DiagnosticSeverity::Error => ('E', GutterMarkerRole::Error),
        DiagnosticSeverity::Warning => ('W', GutterMarkerRole::Warning),
        DiagnosticSeverity::Info => ('I', GutterMarkerRole::Info),
        DiagnosticSeverity::Hint => ('H', GutterMarkerRole::Muted),
    };
    GutterMarkerStyle {
        glyph,
        role,
        modifier: Modifier::BOLD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(source_line: usize, severity: DiagnosticSeverity) -> GutterTroubleItem {
        GutterTroubleItem {
            source_line,
            severity,
        }
    }

    #[test]
    fn shuffled_duplicates_keep_explicit_highest_severity() {
        let severities = [
            DiagnosticSeverity::Error,
            DiagnosticSeverity::Warning,
            DiagnosticSeverity::Info,
            DiagnosticSeverity::Hint,
        ];
        for first in severities {
            for second in severities {
                for pair in [[first, second], [second, first]] {
                    let items = [
                        item(9, pair[0]),
                        item(2, DiagnosticSeverity::Hint),
                        item(9, pair[1]),
                    ];
                    let mut build = PendingGutterTroubleBuild::new(7, items.len());
                    assert_eq!(
                        build.advance(usize::MAX, |index| Some(items[index])),
                        items.len()
                    );
                    let snapshot = build.publish(7).unwrap();
                    let expected = if severity_priority(first) >= severity_priority(second) {
                        first
                    } else {
                        second
                    };
                    assert_eq!(snapshot.severity(9), Some(expected));
                    assert_eq!(snapshot.severity(2), Some(DiagnosticSeverity::Hint));
                    assert_eq!(snapshot.len(), 2);
                }
            }
        }
    }

    #[test]
    fn marker_glyph_role_and_modifier_mapping_is_exhaustive() {
        for (severity, glyph, role) in [
            (DiagnosticSeverity::Error, 'E', GutterMarkerRole::Error),
            (DiagnosticSeverity::Warning, 'W', GutterMarkerRole::Warning),
            (DiagnosticSeverity::Info, 'I', GutterMarkerRole::Info),
            (DiagnosticSeverity::Hint, 'H', GutterMarkerRole::Muted),
        ] {
            assert_eq!(
                marker_style(severity),
                GutterMarkerStyle {
                    glyph,
                    role,
                    modifier: Modifier::BOLD,
                }
            );
        }
    }

    #[test]
    fn budget_completion_and_generation_publication_are_exact() {
        let items = [
            item(1, DiagnosticSeverity::Hint),
            item(2, DiagnosticSeverity::Info),
            item(3, DiagnosticSeverity::Error),
        ];
        let mut build = PendingGutterTroubleBuild::new(11, items.len());
        assert_eq!(build.advance(0, |index| Some(items[index])), 0);
        assert_eq!(build.mapped_items(), 0);
        assert_eq!(build.advance(1, |index| Some(items[index])), 1);
        assert_eq!(build.mapped_items(), 1);
        assert!(!build.is_complete());
        assert_eq!(build.advance(1, |index| Some(items[index])), 1);
        assert_eq!(build.mapped_items(), 2);
        assert!(!build.is_complete());
        assert_eq!(build.advance(50, |index| Some(items[index])), 1);
        assert_eq!(build.mapped_items(), 3);
        assert!(build.is_complete());
        assert_eq!(build.publish(11).unwrap().len(), 3);

        let mut partial = PendingGutterTroubleBuild::new(11, items.len());
        partial.advance(2, |index| Some(items[index]));
        assert!(partial.publish(11).is_none());

        let mut stale = PendingGutterTroubleBuild::new(11, items.len());
        stale.advance(usize::MAX, |index| Some(items[index]));
        assert!(stale.publish(12).is_none());
    }

    #[test]
    fn snapshot_memory_is_compact_sorted_and_lookup_is_read_only() {
        let count = 10_000;
        let items: Vec<_> = (0..count)
            .rev()
            .map(|source_line| item(source_line, DiagnosticSeverity::Warning))
            .collect();
        let mut build = PendingGutterTroubleBuild::new(3, items.len());
        build.advance(count, |index| Some(items[index]));
        let snapshot = build.publish(3).unwrap();
        assert_eq!(snapshot.len(), count);
        assert!(!snapshot.is_empty());
        assert!(std::mem::size_of::<GutterTroubleEntry>() <= 24);
        assert!(snapshot.heap_capacity_bytes() <= count * 24 + 4_096);
        let capacity = snapshot.heap_capacity_bytes();
        for source_line in 0..count {
            assert_eq!(
                snapshot.severity(source_line),
                Some(DiagnosticSeverity::Warning)
            );
        }
        assert_eq!(snapshot.severity(count), None);
        assert_eq!(snapshot.heap_capacity_bytes(), capacity);
    }
}
