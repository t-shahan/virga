use crate::app::App;
use crate::theme::Palette;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

use super::centered;
use super::legend::bindings;

/// The full key reference, one binding per line, drawn over the content.
///
/// A vertical list rather than the bar's packed rows, on purpose: the bar's
/// wrapping tops out at two rows and drops bindings from the tail, which is
/// the right trade for a hint and the wrong one for the reference — a card
/// that omits keys at narrow widths answers the exact question it was opened
/// for with silence. One line each fits the floor almost exactly: the
/// longest list is one line taller than the minimum terminal height can hold
/// with its border, and the one line that clips there is `[?] keys` — the one
/// binding the footer, "any key closes", already restates.
pub(super) fn help_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    let bindings = bindings(app);

    // Keys are right-aligned in a shared column so the labels rank up,
    // whatever mix of `[q]` and `[←→↑↓]` sits above them.
    let key_col = bindings
        .iter()
        .map(|(key, _)| key.chars().count() + 2)
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = bindings
        .iter()
        .map(|(key, label)| {
            let key = format!("[{key}]");
            Line::from(vec![
                Span::from(" ".repeat(key_col.saturating_sub(key.chars().count()))),
                Span::from(key).fg(palette.selection),
                Span::from(format!(" {label}")).fg(palette.muted),
            ])
        })
        .collect();

    let width = lines.iter().map(Line::width).max().unwrap_or(0) as u16;
    // Taller than the terminal clips from the tail, which the ordering makes
    // the right end to lose: the way out is the first line, not the last.
    let height = (lines.len() as u16).saturating_add(2).min(area.height);
    let card = centered(area, width.saturating_add(4).min(area.width), height);

    frame.render_widget(Clear, card);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .padding(Padding::horizontal(1))
                .border_style(Style::new().fg(palette.border))
                // A block title takes the block's own style, not the border's,
                // so left alone it would render in the terminal's default
                // foreground — invisible on a themed ground.
                .title(Line::from("Keys").fg(palette.text))
                .title_bottom(Line::from("any key closes").fg(palette.muted).centered()),
        ),
        card,
    );
}
