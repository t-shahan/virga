use crate::app::{App, Fetch};
use crate::theme::Palette;
use crate::ui::{centered, clear_to_ground, spinner};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

pub(super) fn search_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    let area = centered(area, 50, 12);
    clear_to_ground(frame, area, palette);

    let block = Block::bordered()
        // Styled explicitly: a block title takes the block's own style rather
        // than the border's, so an unstyled one keeps the terminal's default
        // foreground whatever the theme says.
        .title(Line::from("Search").fg(palette.muted))
        .border_style(Style::new().fg(palette.border));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [input_area, list_area] = Layout::vertical([
        Constraint::Length(2), // text row + underline
        Constraint::Fill(1),
    ])
    .areas(inner);

    let input_block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(palette.muted));
    let input_inner = input_block.inner(input_area);
    frame.render_widget(input_block, input_area);

    let input_line = if app.query.is_empty() {
        Line::from(vec![
            Span::from("❯ ").fg(palette.accent),
            Span::from("search for a city").fg(palette.muted).italic(),
        ])
    } else {
        let cursor = if (app.tick / 5).is_multiple_of(2) {
            "▏"
        } else {
            " "
        };
        Line::from(vec![
            Span::from("❯ ").fg(palette.accent),
            Span::from(app.query.as_str()).fg(palette.text),
            Span::from(cursor).fg(palette.accent),
        ])
    };

    frame.render_widget(Paragraph::new(input_line), input_inner);

    let body: Vec<Line> = match &app.results {
        Fetch::Idle => Vec::new(),
        // Both of these were `.dim()` and nothing else, which left them on the
        // terminal's default foreground: on a themed ground they neither
        // repainted nor stayed reliably readable. `muted` is the role for text
        // that is meant to recede, and it carries the dimming itself — stacking
        // DIM on top of a colour already chosen to be quiet is what makes a
        // status message unreadable rather than merely secondary.
        Fetch::Loading => {
            vec![Line::from(format!("{} searching...", spinner(app.tick))).fg(palette.muted)]
        }
        Fetch::Ready(locations) if locations.is_empty() => {
            vec![Line::from("no matches").fg(palette.muted)]
        }
        Fetch::Ready(locations) => locations
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let marker = if i == app.selected { ">" } else { " " };
                let text = format!("{marker} {}", l.label());
                if i == app.selected {
                    Line::from(text).fg(palette.selection).bold()
                } else {
                    Line::from(text).fg(palette.accent)
                }
            })
            .collect(),
        Fetch::Failed(e) => vec![Line::from(format!("error: {e}")).fg(palette.error)],
    };

    frame.render_widget(Paragraph::new(body), list_area);
}
