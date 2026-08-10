use crate::app::{App, Fetch, Screen};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::widgets::{Block, Clear, Paragraph};

mod chart;
mod current;
mod digits;
mod forecast;
mod legend;
mod search;

use chart::chart_area_render;
use current::current_area_render;
use forecast::forecast_area_render;
use legend::keybind_legend_render;
use search::search_render;

/// Shown wherever the API reported no value for a reading.
const UNKNOWN: &str = "unavailable";

/// Columns between the table and the chart when they sit side by side.
const GUTTER: u16 = 3;
/// Below this they stack instead. Splitting only pays when the table keeps
/// every column *and* the chart keeps readable bars, so the threshold is
/// composed from what each half needs rather than guessed at.
const SIDE_BY_SIDE_MIN: u16 = forecast::TABLE_FULL + GUTTER + chart::COMFORTABLE_WIDTH;

const _: () = assert!(SIDE_BY_SIDE_MIN > forecast::TABLE_FULL + GUTTER);

/// Below this even the detail column clips mid-word, so say so plainly rather
/// than rendering a broken layout. Deliberately generous: an earlier attempt
/// set this from the *comfortable* layout and rejected an ordinary 100x20.
const MIN_WIDTH: u16 = 34;
const MIN_HEIGHT: u16 = 12;

pub(crate) fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        too_small_render(frame, area);
        return;
    }

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
                    Layout::vertical([Constraint::Length(8), Constraint::Fill(1)]).areas(content);

                // Table and chart are separate boxes now. Side by side buys
                // rows at the cost of chart width, so it is a fallback for
                // short windows rather than a reward for wide ones.
                let table_rows = w.daily.len().saturating_sub(w.today_index) as u16 + 1;
                let table_box = table_rows + 2;
                let side_by_side = rest.width >= SIDE_BY_SIDE_MIN
                    && rest.height < table_box + chart::MIN_HEIGHT + 2;

                let (forecast_area, chart_area) = if side_by_side {
                    let [left, _gutter, right] = Layout::horizontal([
                        Constraint::Length(forecast::TABLE_FULL + 2),
                        Constraint::Length(GUTTER),
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
                            height: chart.height.min(chart::MAX_HEIGHT + 2),
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

fn too_small_render(frame: &mut Frame, area: Rect) {
    let message = format!(
        "Terminal too small\n\n{}x{}\nneeds {MIN_WIDTH}x{MIN_HEIGHT}",
        area.width, area.height
    );

    // No border: at these sizes it would cost two of the few rows there are.
    frame.render_widget(
        Paragraph::new(message).alignment(Alignment::Center),
        centered(area, area.width.min(MIN_WIDTH), 4.min(area.height)),
    );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// A terminal below the minimum must say so rather than render a clipped
    /// mess — and must not panic doing it, including at one cell.
    #[test]
    fn tiny_terminals_get_a_message_instead_of_a_broken_layout() {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));

        for (width, height) in [(1, 1), (10, 5), (33, 11), (34, 11), (33, 12)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| render(f, &app)).unwrap();

            let buf = t.backend().buffer();
            let text: String = (0..height)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| buf[(x, y)].symbol())
                .collect();
            assert!(
                !text.contains("feels like"),
                "{width}x{height} rendered the pane anyway"
            );
        }
    }

    #[test]
    fn the_minimum_size_itself_renders_the_app() {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));

        let mut t = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).unwrap();
        t.draw(|f| render(f, &app)).unwrap();

        let buf = t.backend().buffer();
        let text: String = (0..MIN_HEIGHT)
            .flat_map(|y| (0..MIN_WIDTH).map(move |x| (x, y)))
            .map(|(x, y)| buf[(x, y)].symbol())
            .collect();
        assert!(text.contains("feels like"), "the minimum should be usable");
    }
}
