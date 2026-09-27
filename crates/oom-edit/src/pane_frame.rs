//! Owned, pane-local presentation cells for embedding hosts.

use ratatui::buffer::{Buffer, CellWidth};
use ratatui::style::Style;
use std::sync::Arc;

use crate::owned_style::OwnedStyle;

/// One visual cell, including a marker for the hidden half of a wide glyph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneCell {
    /// A complete grapheme at its leading cell, or a space at a continuation.
    pub symbol: String,
    /// Fully resolved foreground, background, underline color and modifiers.
    pub style: OwnedStyle,
    /// This cell is occupied by the preceding wide grapheme and must not print.
    pub continuation: bool,
}

impl Default for PaneCell {
    fn default() -> Self {
        Self {
            symbol: " ".to_string(),
            style: OwnedStyle::default(),
            continuation: false,
        }
    }
}

/// Terminal cursor shape requested by the editor's current mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneCursorShape {
    /// Steady block cursor.
    Block,
    /// Steady vertical-bar cursor.
    Bar,
    /// Steady underline cursor.
    Underscore,
}

/// A visible cursor in pane-local cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneCursor {
    /// Zero-based pane-local display column.
    pub column: u16,
    /// Zero-based pane-local display row.
    pub row: u16,
    /// Mode-specific terminal cursor shape.
    pub shape: PaneCursorShape,
}

/// A complete, owned pane frame. The host applies its screen origin once when
/// copying these cells to its own rendering surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneFrame {
    /// Frame width in terminal cells.
    pub width: u16,
    /// Frame height in terminal cells.
    pub height: u16,
    /// Immutable row-major snapshot; exactly `width * height` entries. Cloning
    /// a frame shares this owned snapshot without copying every grapheme.
    pub cells: Arc<Vec<PaneCell>>,
    /// Visible pane-local cursor, or none when unfocused/hidden.
    pub cursor: Option<PaneCursor>,
    /// Whether cells or cursor differ from the preceding frame supplied by the host.
    pub changed: bool,
}

impl PaneFrame {
    /// Create an empty frame, including the zero-size case.
    pub fn blank(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            cells: Arc::new(vec![
                PaneCell::default();
                usize::from(width) * usize::from(height)
            ]),
            cursor: None,
            changed: true,
        }
    }

    /// Read a cell by pane-local coordinates.
    pub fn cell(&self, column: u16, row: u16) -> Option<&PaneCell> {
        if column >= self.width || row >= self.height {
            return None;
        }
        self.cells
            .get(usize::from(row) * usize::from(self.width) + usize::from(column))
    }

    /// Compare only what a host can show, excluding the change indicator.
    pub fn visually_equals(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.cells == other.cells
            && self.cursor == other.cursor
    }

    /// Render a safe local notice when document/overlay controls cannot fit.
    pub(crate) fn too_small(width: u16, height: u16) -> Self {
        let mut frame = Self::blank(width, height);
        let notice = "Pane too small";
        let count = usize::from(width).min(notice.len());
        let row = height / 2;
        let start = width.saturating_sub(count as u16) / 2;
        for (offset, letter) in notice.chars().take(count).enumerate() {
            if let Some(cell) = Arc::make_mut(&mut frame.cells)
                .get_mut(usize::from(row) * usize::from(width) + usize::from(start) + offset)
            {
                cell.symbol = letter.to_string();
            }
        }
        frame
    }

    pub(crate) fn from_buffer(buffer: &Buffer, cursor: Option<PaneCursor>) -> Self {
        let width = buffer.area.width;
        let height = buffer.area.height;
        let mut cells = Vec::with_capacity(usize::from(width) * usize::from(height));
        for row in 0..height {
            let mut remaining_continuations = 0_u16;
            for column in 0..width {
                let source = &buffer[(column, row)];
                let continuation = remaining_continuations > 0;
                if continuation {
                    remaining_continuations -= 1;
                }
                let symbol = if continuation {
                    " ".to_string()
                } else {
                    let glyph_width = source.symbol().cell_width();
                    if glyph_width > width.saturating_sub(column) {
                        // A malformed final cell must not ask a host to print
                        // half a wide grapheme outside the pane.
                        " ".to_string()
                    } else {
                        remaining_continuations = glyph_width.saturating_sub(1);
                        source.symbol().to_string()
                    }
                };
                let style = Style::default()
                    .fg(source.fg)
                    .bg(source.bg)
                    .underline_color(source.underline_color)
                    .add_modifier(source.modifier);
                cells.push(PaneCell {
                    symbol,
                    style: OwnedStyle::from_ratatui(style),
                    continuation,
                });
            }
        }
        Self {
            width,
            height,
            cells: Arc::new(cells),
            cursor: cursor.filter(|position| position.column < width && position.row < height),
            changed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    use super::*;

    #[test]
    fn conversion_preserves_graphemes_continuations_and_clips_half_wide_glyphs() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 6, 1));
        buffer.set_string(0, 0, "界e\u{301}🙂", Style::default());
        let frame = PaneFrame::from_buffer(&buffer, None);
        assert_eq!(frame.cell(0, 0).unwrap().symbol, "界");
        assert!(frame.cell(1, 0).unwrap().continuation);
        assert_eq!(frame.cell(2, 0).unwrap().symbol, "e\u{301}");
        assert_eq!(frame.cell(3, 0).unwrap().symbol, "🙂");
        assert!(frame.cell(4, 0).unwrap().continuation);
        assert_eq!(frame.cell(5, 0).unwrap().symbol, " ");

        let mut clipped = Buffer::empty(Rect::new(0, 0, 1, 1));
        clipped[(0, 0)].set_symbol("界");
        assert_eq!(
            PaneFrame::from_buffer(&clipped, None)
                .cell(0, 0)
                .unwrap()
                .symbol,
            " "
        );
    }
}
