use crate::app::{App, Fetch, Screen};
use crate::theme::Palette;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

mod bars;
mod chart;
mod current;
mod digits;
mod forecast;
mod legend;
mod precip;
mod precip_chart;
mod search;

use chart::chart_area_render;
use current::current_area_render;
use forecast::forecast_area_render;
use legend::{keybind_legend_render, legend_rows};
use precip::precip_render;
use search::search_render;

/// Shown wherever the API reported no value for a reading.
const UNKNOWN: &str = "unavailable";

/// Columns kept clear between two titles sharing one border row.
const TITLE_GUTTER: usize = 3;

/// Room a border row has for titles: two corners, plus a column of breathing
/// space inside each.
fn title_room(width: u16) -> usize {
    width.saturating_sub(4) as usize
}

/// Clip to `width` on a character boundary, marking the cut so a truncated
/// value cannot be mistaken for a short one.
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

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
    render_with(frame, app, app.theme.palette());
}

/// The frame, drawn in a given palette.
///
/// The theme is resolved exactly once, here, and handed to every widget as a
/// value — no widget reaches for `app.theme` itself. That is what lets a test
/// render the whole app in a palette of its own choosing and check that no
/// colour survived being hard-coded.
fn render_with(frame: &mut Frame, app: &App, palette: Palette) {
    let area = frame.area();

    // Before anything else, and over the whole frame including the rows the
    // legend will take. A palette that sets only foregrounds is a palette that
    // is correct on a dark terminal and unreadable on a light one.
    ground(frame, area, palette);

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        too_small_render(frame, area, palette);
        return;
    }

    // The legend is pinned to the bottom; everything else is laid out inside
    // what remains so panes can be sized to their content. It asks for its own
    // height, because a narrow terminal wraps it onto a second row rather than
    // clipping bindings mid-word.
    let [content, legend_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(legend_rows(app, area.width)),
    ])
    .areas(area);

    match app.screen {
        Screen::Weather => match &app.weather {
            Fetch::Ready(w) => {
                // City and day live in the pane's border now, so the separate
                // header box is gone and its rows go to the chart.
                let [current_area, rest] =
                    Layout::vertical([Constraint::Length(7), Constraint::Fill(1)]).areas(content);

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

                current_area_render(frame, app, w, palette, current_area);
                forecast_area_render(frame, w, palette, forecast_area, app.unit, app.selected_day);
                chart_area_render(frame, w, palette, chart_area, app.unit, app.selected_day);
            }
            Fetch::Loading => popup_render(
                frame,
                area,
                palette,
                "Loading",
                &format!("{} fetching...", spinner(app.tick)),
            ),
            Fetch::Failed(msg) => popup_render(frame, area, palette, "Error", msg),
            Fetch::Idle => {}
        },
        Screen::Precipitation => match &app.weather {
            Fetch::Ready(_) => precip_render(frame, app, palette, content),
            Fetch::Loading => popup_render(
                frame,
                area,
                palette,
                "Loading",
                &format!("{} fetching...", spinner(app.tick)),
            ),
            Fetch::Failed(msg) => popup_render(frame, area, palette, "Error", msg),
            Fetch::Idle => {}
        },
        Screen::Search => search_render(frame, app, palette, area),
    }
    keybind_legend_render(frame, app, palette, legend_area);
}

fn too_small_render(frame: &mut Frame, area: Rect, palette: Palette) {
    let message = format!(
        "Terminal too small\n\n{}x{}\nneeds {MIN_WIDTH}x{MIN_HEIGHT}",
        area.width, area.height
    );

    // No border: at these sizes it would cost two of the few rows there are.
    frame.render_widget(
        Paragraph::new(message)
            .style(Style::new().fg(palette.text))
            .alignment(Alignment::Center),
        centered(area, area.width.min(MIN_WIDTH), 4.min(area.height)),
    );
}

/// Lay the palette's ground over `area`, leaving whatever is already drawn
/// there alone: only the background is patched, so this is safe to call before
/// the content and only affects cells the theme is entitled to.
fn ground(frame: &mut Frame, area: Rect, palette: Palette) {
    frame
        .buffer_mut()
        .set_style(area, Style::new().bg(palette.background));
}

/// `Clear` and then the ground put back.
///
/// `Clear` resets a cell wholesale — style as well as symbol — so a popup or
/// the search box would otherwise punch a terminal-coloured hole through a
/// themed background, which is exactly the kind of gap the ground was added to
/// close.
pub(super) fn clear_to_ground(frame: &mut Frame, area: Rect, palette: Palette) {
    frame.render_widget(Clear, area);
    ground(frame, area, palette);
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

fn popup_render(frame: &mut Frame, area: Rect, palette: Palette, title: &str, body: &str) {
    let area = centered(area, 40, 5);

    // An error is the one message the reader has to act on, so it takes the
    // error colour while the loading spinner stays ordinary text.
    let colour = if title == "Error" {
        palette.error
    } else {
        palette.text
    };

    clear_to_ground(frame, area, palette);
    frame.render_widget(
        Paragraph::new(body)
            .style(Style::new().fg(colour))
            .alignment(Alignment::Center)
            .block(
                Block::bordered()
                    .border_style(Style::new().fg(palette.border))
                    // A block title takes the block's own style, not the
                    // border's, so left alone it renders in the terminal's
                    // default foreground — invisible on a themed ground.
                    .title(Line::from(title).fg(colour)),
            ),
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
    use crate::theme::Theme;
    use crate::weather::model::{Location, Weather};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Color;

    /// Sizes that between them exercise every branch of the layout: side by
    /// side, stacked, the minimum the app will draw at, and one below it,
    /// where the size warning replaces the interface entirely and is otherwise
    /// never seen by the palette sweeps.
    const SIZES: [(u16, u16); 5] = [
        (120, 40),
        (100, 20),
        (60, 24),
        (MIN_WIDTH, MIN_HEIGHT),
        (20, 8),
    ];

    fn ready(screen: Screen) -> App {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.screen = screen;
        app
    }

    fn found() -> Vec<Location> {
        vec![
            Location {
                name: "Frederick".to_string(),
                admin1: Some("Maryland".to_string()),
                country: Some("United States".to_string()),
                lat: 39.414_27,
                lon: -77.410_54,
            },
            Location {
                name: "Fredericksburg".to_string(),
                admin1: Some("Virginia".to_string()),
                country: Some("United States".to_string()),
                lat: 38.301_8,
                lon: -77.460_5,
            },
        ]
    }

    /// Every state the frame can be drawn in, named.
    ///
    /// A sweep is only as good as the branches it reaches: the previous one
    /// rendered ready and failed and nothing else, so two search statuses that
    /// never asked the palette for a colour sat behind it unnoticed. Anything
    /// `render_with` can put on screen belongs in here.
    fn states() -> Vec<(String, App)> {
        let mut states = Vec::new();

        for screen in [Screen::Weather, Screen::Precipitation] {
            for name in ["ready", "loading", "failed", "idle"] {
                let mut app = ready(screen);
                app.weather = match name {
                    "loading" => Fetch::Loading,
                    "failed" => Fetch::Failed("the network went away".to_string()),
                    "idle" => Fetch::Idle,
                    _ => app.weather,
                };
                states.push((format!("{screen:?}/{name}"), app));
            }
        }

        // The search box floats over a loaded screen, so the weather stays
        // ready underneath: whatever it draws has to survive being cleared.
        for (name, query, results) in [
            ("prompt", "", Fetch::Idle),
            ("typing", "freder", Fetch::Idle),
            ("searching", "freder", Fetch::Loading),
            ("no matches", "freder", Fetch::Ready(Vec::new())),
            ("results", "freder", Fetch::Ready(found())),
            (
                "failed",
                "freder",
                Fetch::Failed("the network went away".to_string()),
            ),
        ] {
            let mut app = ready(Screen::Search);
            app.query = query.to_string();
            app.results = results;
            states.push((format!("Search/{name}"), app));
        }

        states
    }

    fn drawn(app: &App, palette: Palette, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| render_with(f, app, palette))
            .unwrap()
            .buffer
            .clone()
    }

    fn symbols(buffer: &Buffer, width: u16, height: u16) -> Vec<String> {
        (0..height)
            .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
            .collect()
    }

    /// Nine colours that appear in no palette, one per role. Rendering in these
    /// makes every painted cell traceable back to the role that painted it —
    /// and any cell wearing something else a call site that never got a
    /// palette.
    fn probe() -> Palette {
        Palette {
            background: Color::Rgb(0, 9, 0),
            accent: Color::Rgb(1, 0, 0),
            text: Color::Rgb(2, 0, 0),
            muted: Color::Rgb(3, 0, 0),
            selection: Color::Rgb(4, 0, 0),
            now: Color::Rgb(5, 0, 0),
            series: Color::Rgb(6, 0, 0),
            error: Color::Rgb(7, 0, 0),
            border: Color::Rgb(8, 0, 0),
        }
    }

    /// The foreground roles, for checking a cell's colour came from one of them.
    fn roles(palette: Palette) -> [Color; 8] {
        [
            palette.accent,
            palette.text,
            palette.muted,
            palette.selection,
            palette.now,
            palette.series,
            palette.error,
            palette.border,
        ]
    }

    /// The test that makes the feature real rather than mostly-real. Before
    /// themes there were around thirty colour literals scattered across seven
    /// modules; a single one left behind is a widget that ignores the theme,
    /// and reading the diff is not a reliable way to know they all moved.
    ///
    /// Stated as what a cell *must* be rather than as a list of colours it must
    /// not be. The list was the first version and it was too weak twice over:
    /// it passed any literal that happened not to be one of the six, and it
    /// passed a glyph with no colour at all — which is precisely what the two
    /// search statuses were, and why they went unnoticed.
    ///
    /// The alternative of grepping the source for colour literals was
    /// considered and rejected: it cannot tell a colour that reaches the screen
    /// from one that does not, and `theme.rs` would have to be excepted from
    /// its own rule.
    #[test]
    fn no_widget_keeps_a_colour_of_its_own() {
        let probe = probe();
        let roles = roles(probe);

        for (state, app) in states() {
            for (width, height) in SIZES {
                let buffer = drawn(&app, probe, width, height);

                for y in 0..height {
                    for x in 0..width {
                        let cell = &buffer[(x, y)];
                        let where_ = format!("{state} at {width}x{height}: cell ({x}, {y})");

                        // `Color::Reset` is what an unstyled cell carries, so
                        // it reads as "nobody painted this" rather than as a
                        // colour — hence the second assertion below.
                        assert!(
                            cell.fg == Color::Reset || roles.contains(&cell.fg),
                            "{where_} is {:?}, which is no role in the palette",
                            cell.fg
                        );

                        assert!(
                            cell.symbol().trim().is_empty() || cell.fg != Color::Reset,
                            "{where_} draws {:?} without asking the palette for a colour",
                            cell.symbol()
                        );
                    }
                }
            }
        }
    }

    /// The palettes are all designed against a dark ground, so the theme has to
    /// bring one: left to the terminal, Nord text sits at about 1.15:1 on a
    /// light scheme. Every cell, in every state — `Clear` resets a cell's style
    /// along with its symbol, so the search box and the popups are the two
    /// places the ground can be punched back out.
    #[test]
    fn the_ground_reaches_every_cell() {
        let probe = probe();

        for (state, app) in states() {
            for (width, height) in SIZES {
                let buffer = drawn(&app, probe, width, height);

                for y in 0..height {
                    for x in 0..width {
                        assert_eq!(
                            buffer[(x, y)].bg,
                            probe.background,
                            "{state} at {width}x{height}: cell ({x}, {y}) \
                             kept the terminal's own ground"
                        );
                    }
                }
            }
        }
    }

    /// A palette may change what a cell looks like but never which cell it is.
    /// If colour could move the layout, every size-related guarantee in this
    /// module would hold for the default theme only.
    ///
    /// The app is held fixed and only the palette varies, so the legend's
    /// theme readout — the one place a theme legitimately changes the glyphs —
    /// does not mask the thing being tested. That the readout itself always
    /// fits is `legend`'s business, and it sweeps every name to prove it.
    #[test]
    fn a_palette_never_moves_a_cell() {
        for (state, app) in states() {
            for (width, height) in SIZES {
                let reference = symbols(
                    &drawn(&app, Theme::default().palette(), width, height),
                    width,
                    height,
                );

                for theme in Theme::ALL {
                    assert_eq!(
                        symbols(&drawn(&app, theme.palette(), width, height), width, height),
                        reference,
                        "{} moved the layout on {state} at {width}x{height}",
                        theme.name()
                    );
                }
            }
        }
    }

    /// The other half of the pair: proof the palette reaches the screen at
    /// all, so `every_theme_draws_the_same_layout` cannot be satisfied by a
    /// theme that simply does nothing.
    #[test]
    fn changing_the_theme_repaints_the_screen() {
        let app = ready(Screen::Weather);
        let reference = drawn(&app, Theme::default().palette(), 120, 40);

        for theme in Theme::ALL.into_iter().skip(1) {
            let themed = drawn(&app, theme.palette(), 120, 40);
            let repainted = (0..40u16)
                .flat_map(|y| (0..120u16).map(move |x| (x, y)))
                .any(|cell| themed[cell].style().fg != reference[cell].style().fg);

            assert!(repainted, "{} left the screen unchanged", theme.name());
        }
    }

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
