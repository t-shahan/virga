use crate::app::{App, Fetch, Screen};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::widgets::{Block, Clear, Paragraph};

mod current;
mod forecast;
mod legend;
mod search;
mod top;

use current::current_area_render;
use forecast::forecast_area_render;
use legend::keybind_legend_render;
use search::search_render;
use top::top_area_render;

/// Shown wherever the API reported no value for a reading.
const UNKNOWN: &str = "unavailable";

pub(crate) fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    let [top_area, current_area, forecast_area, legend_area] = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(7),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    match app.screen {
        Screen::Weather => match &app.weather {
            Fetch::Ready(w) => {
                top_area_render(frame, app, w, top_area);
                current_area_render(frame, w, current_area, app.unit);
                forecast_area_render(frame, w, forecast_area, app.unit);
            }
            Fetch::Loading => popup_render(
                frame,
                area,
                "Loading",
                &format!("{} fetching...", spinner(app.tick)),
            ),
            Fetch::Failed(msg) => popup_render(frame, area, "Error", msg),
            Fetch::Idle => {}
        },
        Screen::Search => search_render(frame, app, area),
    }
    keybind_legend_render(frame, app, legend_area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [area] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    area
}

fn popup_render(frame: &mut Frame, area: Rect, title: &str, body: &str) {
    let area = centered(area, 40, 5);

    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(body)
            .alignment(Alignment::Center)
            .block(Block::bordered().title(title)),
        area,
    )
}

fn spinner(tick: usize) -> &'static str {
    const FRAMES: [&str; 4] = ["|", "/", "-", "\\"];
    FRAMES[(tick / 2) % FRAMES.len()]
}
