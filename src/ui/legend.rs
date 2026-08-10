use crate::app::{App, Fetch, Screen};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub(super) fn keybind_legend_render(frame: &mut Frame, app: &App, area: Rect) {
    let binds: &[(&str, &str)] = match app.screen {
        Screen::Weather => &[
            ("q", "quit"),
            ("←→", "day"),
            ("n", "now"),
            ("r", "refresh"),
            ("u", "units"),
            ("l", "location"),
        ],
        Screen::Search => match &app.results {
            Fetch::Ready(l) if !l.is_empty() => {
                &[("↑↓", "navigate"), ("enter", "select"), ("esc", "cancel")]
            }
            _ => &[("enter", "search"), ("esc", "cancel")],
        },
    };

    let mut spans = vec![Span::from("  ")];
    for (key, label) in binds {
        spans.push(Span::from(format!("[{key}]")).yellow());
        spans.push(Span::from(format!(" {label}   ")).dark_gray());
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
