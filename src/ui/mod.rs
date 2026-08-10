use crate::app::{App, Fetch, Screen};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::widgets::{Block, Clear, Paragraph};

mod current;
mod forecast;
mod legend;
mod search;

use current::current_area_render;
use forecast::{chart_area_render, forecast_area_render};
use legend::keybind_legend_render;
use search::search_render;

/// Shown wherever the API reported no value for a reading.
const UNKNOWN: &str = "unavailable";

pub(crate) fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // The legend is pinned to the bottom; everything else is laid out inside
    // what remains so panes can be sized to their content.
    let [content, legend_area] =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area);

    match app.screen {
        Screen::Weather => match &app.weather {
            Fetch::Ready(w) => {
                // City and day live in the pane's border now, so the separate
                // header box is gone and its rows go to the chart.
                let [current_area, rest] =
                    Layout::vertical([Constraint::Length(9), Constraint::Fill(1)]).areas(content);

                // Table and chart are separate boxes now. Side by side buys
                // rows at the cost of chart width, so it is a fallback for
                // short windows rather than a reward for wide ones.
                let table_rows = w.daily.len().saturating_sub(w.today_index) as u16 + 1;
                let table_box = table_rows + 2;
                let side_by_side = rest.width >= forecast::SIDE_BY_SIDE_MIN
                    && rest.height < table_box + forecast::CHART_MIN + 2;

                let (forecast_area, chart_area) = if side_by_side {
                    let [left, _gutter, right] = Layout::horizontal([
                        Constraint::Length(forecast::TABLE_FULL + 2),
                        Constraint::Length(forecast::GUTTER),
                        Constraint::Fill(1),
                    ])
                    .areas(rest);
                    (
                        Rect {
                            height: table_box.min(left.height),
                            ..left
                        },
                        Rect {
                            height: table_box.min(right.height),
                            ..right
                        },
                    )
                } else {
                    let [table, chart] =
                        Layout::vertical([Constraint::Length(table_box), Constraint::Fill(1)])
                            .areas(rest);
                    (
                        table,
                        Rect {
                            height: chart.height.min(forecast::CHART_MAX + 2),
                            ..chart
                        },
                    )
                };

                current_area_render(frame, app, w, current_area);
                forecast_area_render(frame, w, forecast_area, app.unit, app.selected_day);
                chart_area_render(frame, w, chart_area, app.unit, app.selected_day);
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
