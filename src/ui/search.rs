use crate::app::{App, Fetch};
use crate::ui::{centered, spinner};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

pub(super) fn search_render(frame: &mut Frame, app: &App, area: Rect) {
    let area = centered(area, 50, 12);
    frame.render_widget(Clear, area);

    let block = Block::bordered().title("Search");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [input_area, list_area] = Layout::vertical([
        Constraint::Length(2), // text row + underline
        Constraint::Fill(1),
    ])
    .areas(inner);

    let input_block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::new().fg(Color::DarkGray));
    let input_inner = input_block.inner(input_area);
    frame.render_widget(input_block, input_area);

    let input_line = if app.query.is_empty() {
        Line::from(vec![
            Span::from("❯ ").cyan(),
            Span::from("search for a city").dark_gray().italic(),
        ])
    } else {
        let cursor = if (app.tick / 5).is_multiple_of(2) {
            "▏"
        } else {
            " "
        };
        Line::from(vec![
            Span::from("❯ ").cyan(),
            Span::from(app.query.as_str()).white(),
            Span::from(cursor).cyan(),
        ])
    };

    frame.render_widget(Paragraph::new(input_line), input_inner);

    let body: Vec<Line> = match &app.results {
        Fetch::Idle => Vec::new(),
        Fetch::Loading => vec![Line::from(format!("{} searching...", spinner(app.tick))).dim()],
        Fetch::Ready(locations) if locations.is_empty() => {
            vec![Line::from("no matches").dim()]
        }
        Fetch::Ready(locations) => locations
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let marker = if i == app.selected { ">" } else { " " };
                let text = format!("{marker} {}", l.label());
                if i == app.selected {
                    Line::from(text).yellow().bold()
                } else {
                    Line::from(text).cyan()
                }
            })
            .collect(),
        Fetch::Failed(e) => vec![Line::from(format!("error: {e}")).red()],
    };

    frame.render_widget(Paragraph::new(body), list_area);
}
