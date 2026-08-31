use crate::app::{App, Fetch, HourlyView, Screen};
use crate::theme::Palette;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};

mod axis;
mod bars;
mod chart;
mod condition_symbol;
mod current;
mod digits;
mod forecast;
mod help;
mod hourly;
mod legend;
mod precip;
mod precip_chart;
mod precip_week;
mod precipitation;
mod search;
mod weathergram;

use chart::chart_area_render;
use current::current_area_render;
use forecast::forecast_area_render;
use help::help_render;
use hourly::hourly_render;
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
/// The canvas the weather screens render inside. Past the width where the
/// widest content fits — the 48-hour weathergram, the forecast table beside
/// its chart — broader boxes only detach titles and controls from the
/// information they describe, so surplus width becomes symmetric margin
/// instead. The search screen keeps the whole area: it is a picker laid over
/// the app, not a dashboard.
const CANVAS_WIDTH: u16 = 120;

pub(crate) fn render(frame: &mut Frame, app: &App) {
    render_with(frame, app, app.theme.palette_for(app.color_depth));
}

/// The frame, drawn in a given palette.
///
/// The theme is resolved exactly once, here, and handed to every widget as a
/// value — no widget reaches for `app.theme` itself. That is what lets a test
/// render the whole app in a palette of its own choosing and check that no
/// colour survived being hard-coded.
fn render_with(frame: &mut Frame, app: &App, palette: Palette) {
    let area = frame.area();

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        too_small_render(frame, area, palette);
        return;
    }

    let page_area = if app.screen == Screen::Search {
        area
    } else {
        let width = area.width.min(CANVAS_WIDTH);
        Rect {
            x: area.x + (area.width - width) / 2,
            width,
            ..area
        }
    };

    // The release notice takes one muted row above the key bar — but not at
    // the minimum height, where every row is already spoken for, and not on
    // the search screen, which lays itself out over the whole area and where
    // news can wait for the choosing to finish.
    let notice_visible =
        app.update_notice.is_some() && app.screen != Screen::Search && area.height > MIN_HEIGHT;

    // The legend is pinned to the bottom; everything else is laid out inside
    // what remains so panes can be sized to their content. It asks for its own
    // height, because a narrow terminal wraps it onto a second row rather than
    // clipping bindings mid-word.
    let [content, notice_area, legend_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(u16::from(notice_visible)),
        Constraint::Length(legend_rows(app, page_area.width)),
    ])
    .areas(page_area);

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
                // Clamped to the area before the cast, and saturating after
                // it: the count is the server's word, the DTO cap
                // notwithstanding, and a table can never be taller than the
                // space it draws in anyway.
                let days = w
                    .daily
                    .len()
                    .saturating_sub(w.today_index)
                    .min(rest.height as usize) as u16;
                let table_rows = days.saturating_add(1);
                let table_box = table_rows.saturating_add(2);
                let side_by_side = rest.width >= SIDE_BY_SIDE_MIN
                    && rest.height < table_box.saturating_add(chart::MIN_HEIGHT + 2);

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
                loading_title(app),
                &format!("{} {}", spinner(app.tick), loading_verb(app)),
            ),
            Fetch::Failed(msg) => popup_render(frame, area, palette, "Error", msg),
            Fetch::Idle => {}
        },
        Screen::Hourly => match &app.weather {
            Fetch::Ready(_) => match app.hourly_view {
                HourlyView::Weathergram => hourly_render(frame, app, palette, content),
                HourlyView::Classic => precip_render(frame, app, palette, content),
            },
            Fetch::Loading => popup_render(
                frame,
                page_area,
                palette,
                loading_title(app),
                &format!("{} {}", spinner(app.tick), loading_verb(app)),
            ),
            Fetch::Failed(msg) => popup_render(frame, page_area, palette, "Error", msg),
            Fetch::Idle => {}
        },
        Screen::Search => search_render(frame, app, palette, area),
    }
    // Centred like the key bar below it: the two share the bottom of the
    // canvas, and one flush-left row under centred panes read as detached.
    if notice_visible && let Some(notice) = &app.update_notice {
        frame.render_widget(
            Paragraph::new(truncate(notice, notice_area.width as usize))
                .style(Style::default().fg(palette.muted))
                .alignment(Alignment::Center),
            notice_area,
        );
    }
    // The hint's one job is advertising the reference; while the reference is
    // up it has nothing to add, and at the minimum size the card only half
    // covers its row, shearing bindings at the card's edges. Its row stays
    // reserved so closing the card does not reflow the panes.
    if !app.help_visible {
        keybind_legend_render(frame, app, palette, legend_area);
    }

    // Last, so the reference sits over whatever the screen drew. It gets the
    // whole canvas rather than just the content: at the minimum height the
    // hourly list plus its border needs every row there is.
    if app.help_visible {
        help_render(frame, app, palette, page_area);
    }
}

/// The first launch of the day can spend a round trip working out where the
/// user is before it has any weather to ask for. Saying so is the difference
/// between a step and a forecast that is taking suspiciously long.
fn loading_title(app: &App) -> &'static str {
    if app.is_locating() {
        "Locating"
    } else {
        "Loading"
    }
}

fn loading_verb(app: &App) -> &'static str {
    if app.is_locating() {
        "locating..."
    } else {
        "fetching..."
    }
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

    frame.render_widget(Clear, area);
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
    use crate::theme::{ColorDepth, Theme};
    use crate::units::Unit;
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

    /// The weather screens are measured dashboards, not wallpaper. Panels
    /// and controls stop growing once the widest content fits, while a
    /// narrower terminal still receives every available column. The search
    /// screen is exempt: it lays its picker over the whole area.
    #[test]
    fn weather_screens_share_one_centered_120_column_canvas() {
        for screen in [Screen::Weather, Screen::Hourly] {
            let app = ready(screen);
            for width in [87, 122, 169] {
                let height = 34;
                let buffer = drawn(&app, Theme::default().palette(), width, height);
                let canvas_width = width.min(120);
                let left = (width - canvas_width) / 2;
                let right = left + canvas_width - 1;

                let painted: Vec<u16> = (0..width)
                    .filter(|x| (0..height).any(|y| !buffer[(*x, y)].symbol().trim().is_empty()))
                    .collect();

                assert_eq!(
                    painted.first().copied(),
                    Some(left),
                    "{screen:?} at width {width}"
                );
                assert_eq!(
                    painted.last().copied(),
                    Some(right),
                    "{screen:?} at width {width}"
                );
                assert!(
                    painted.iter().all(|x| (left..=right).contains(x)),
                    "{screen:?} at width {width} painted beyond {left}..={right}"
                );
            }
        }
    }

    fn ready(screen: Screen) -> App {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.screen = screen;
        app
    }

    /// A first run, one moment in: the detection issued and nothing back yet.
    fn locating(screen: Screen) -> App {
        let mut app = App::with_startup(crate::app::Startup {
            location: crate::app::ActiveLocation::default(),
            source: crate::app::LocationSource::Fallback,
            detect: true,
        });
        let _ = app.startup_request();
        app.screen = screen;
        app
    }

    /// A newer release's news has arrived and nothing has been pressed since.
    fn noticed(screen: Screen) -> App {
        let mut app = ready(screen);
        app.update_notice = Some(
            "update: virga 9.9.9 is available (you have 0.2.0) — run `virga update`".to_string(),
        );
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

        for screen in [Screen::Weather, Screen::Hourly] {
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
            states.push((format!("{screen:?}/locating"), locating(screen)));
            states.push((format!("{screen:?}/update-notice"), noticed(screen)));

            let mut helped = ready(screen);
            helped.help_visible = true;
            states.push((format!("{screen:?}/help"), helped));

            let mut full_style = ready(screen);
            full_style.key_hint_style = crate::app::KeyHintStyle::Full;
            states.push((format!("{screen:?}/full-style"), full_style));
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

    /// A first launch can spend a round trip working out where the user is
    /// before it has anywhere to fetch weather for. Both steps are a spinner
    /// over an empty screen, so if they said the same thing the first would
    /// look like a forecast taking suspiciously long.
    #[test]
    fn the_first_step_says_it_is_locating_rather_than_fetching() {
        for screen in [Screen::Weather, Screen::Hourly] {
            let locating = drawn(&locating(screen), probe(), 60, 24);
            let text = symbols(&locating, 60, 24).join("");
            assert!(text.contains("locating"), "{screen:?}: {text}");

            let mut fetching = ready(screen);
            fetching.weather = Fetch::Loading;
            let fetching = drawn(&fetching, probe(), 60, 24);
            let text = symbols(&fetching, 60, 24).join("");
            assert!(text.contains("fetching"), "{screen:?}: {text}");
        }
    }

    /// The bar only hints, so the overlay is now the one place a binding like
    /// `p` or `l` is discoverable at all.
    #[test]
    fn the_help_overlay_lists_every_binding() {
        let mut app = ready(Screen::Weather);
        app.help_visible = true;
        let text = symbols(&drawn(&app, probe(), 120, 30), 120, 30).join("\n");

        for entry in ["[p] hourly", "[←→↑↓] day", "[l] location", "[t] theme"] {
            assert!(text.contains(entry), "the overlay lost {entry}: {text}");
        }
        assert!(text.contains("Keys"), "{text}");

        // And only that screen's bindings: `l` is not bound on the hourly
        // screen, and its card must not claim otherwise.
        let mut app = ready(Screen::Hourly);
        app.help_visible = true;
        let text = symbols(&drawn(&app, probe(), 120, 30), 120, 30).join("\n");
        assert!(text.contains("[v] view"), "{text}");
        assert!(!text.contains("location"), "{text}");
    }

    /// Closed is the resting state, and it must not leak the reference onto
    /// the dashboard.
    #[test]
    fn a_closed_overlay_leaves_the_dashboard_alone() {
        let app = ready(Screen::Weather);
        let text = symbols(&drawn(&app, probe(), 120, 30), 120, 30).join("\n");
        assert!(!text.contains("[p] hourly"), "{text}");
    }

    /// The overlay has to earn its keep at the floor too: every line whole,
    /// no panic, inside 34x12 — on the hourly screen, whose list is longest.
    #[test]
    fn the_help_overlay_survives_the_minimum_terminal() {
        for screen in [Screen::Weather, Screen::Hourly] {
            let mut app = ready(screen);
            app.help_visible = true;
            let rows = symbols(
                &drawn(&app, probe(), MIN_WIDTH, MIN_HEIGHT),
                MIN_WIDTH,
                MIN_HEIGHT,
            );

            for row in &rows {
                assert_eq!(
                    row.matches('[').count(),
                    row.matches(']').count(),
                    "{screen:?} cut a key in half: {row:?}"
                );
            }
            assert!(
                rows.join("\n").contains("[q] quit"),
                "{screen:?} lost the way out: {rows:?}"
            );
        }
    }

    /// The hourly card is now one line taller than the floor can hold with
    /// its border, so exactly one line clips from the tail — and it must be
    /// `[?] keys`, the binding the footer already restates, rather than the
    /// hide toggle just added beside it.
    #[test]
    fn the_floor_clips_the_redundant_question_mark_line_and_keeps_the_rest() {
        let mut app = ready(Screen::Hourly);
        app.help_visible = true;
        let text = symbols(
            &drawn(&app, probe(), MIN_WIDTH, MIN_HEIGHT),
            MIN_WIDTH,
            MIN_HEIGHT,
        )
        .join("\n");

        for entry in ["[q] quit", "[b] back", "[,] hide"] {
            assert!(text.contains(entry), "lost {entry:?}: {text:?}");
        }
        assert!(
            !text.contains("[?] keys"),
            "the redundant line should have clipped: {text:?}"
        );
    }

    /// `Full` style is the pre-overlay bar revived: every binding named on
    /// the bar itself, with no card behind `?` because there is nothing left
    /// for it to hold.
    #[test]
    fn full_style_names_every_binding_on_the_bar_and_opens_no_card() {
        let mut app = ready(Screen::Weather);
        app.key_hint_style = crate::app::KeyHintStyle::Full;
        let text = symbols(&drawn(&app, probe(), 120, 30), 120, 30).join("\n");

        for entry in ["[p] hourly", "[l] location", "[t] theme"] {
            assert!(text.contains(entry), "{text:?}");
        }
        assert!(!text.contains("any key closes"), "{text:?}");
    }

    /// `?` then any key must be a round trip: the overlay borrows the screen,
    /// it does not get to change it.
    #[test]
    fn toggling_help_twice_returns_the_prior_frame() {
        let mut app = ready(Screen::Weather);
        let before = drawn(&app, probe(), 120, 30);

        app.on_action(crate::input::Action::ToggleHelp);
        let open = drawn(&app, probe(), 120, 30);
        assert_ne!(before, open, "opening help drew nothing");

        app.on_action(crate::input::Action::NextDay);
        let after = drawn(&app, probe(), 120, 30);
        assert_eq!(before, after, "help did not put the screen back");
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

    /// Seven colours that appear in no palette, one per role. Rendering in these
    /// makes every painted cell traceable back to the role that painted it —
    /// and any cell wearing something else a call site that never got a
    /// palette.
    fn probe() -> Palette {
        Palette {
            accent: Color::Rgb(1, 0, 0),
            text: Color::Rgb(2, 0, 0),
            muted: Color::Rgb(3, 0, 0),
            selection: Color::Rgb(4, 0, 0),
            now: Color::Rgb(5, 0, 0),
            error: Color::Rgb(7, 0, 0),
            border: Color::Rgb(8, 0, 0),
        }
    }

    /// The foreground roles, for checking a cell's colour came from one of them.
    fn roles(palette: Palette) -> [Color; 7] {
        [
            palette.accent,
            palette.text,
            palette.muted,
            palette.selection,
            palette.now,
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

    /// A theme paints foregrounds and leaves the ground alone.
    ///
    /// An earlier version gave every palette a background of its own and
    /// painted it over the whole frame. It reads fine on a bare terminal and
    /// wrong on a themed one — a carefully configured scheme ends up with a
    /// rectangle of somebody else's idea of dark stamped over it. So the rule
    /// is that no cell may carry a background at all, and `Color::Reset` is
    /// what a cell nobody has touched carries.
    #[test]
    fn no_theme_paints_a_background() {
        for (state, app) in states() {
            for theme in Theme::ALL {
                for (width, height) in SIZES {
                    let buffer = drawn(&app, theme.palette(), width, height);

                    for y in 0..height {
                        for x in 0..width {
                            assert_eq!(
                                buffer[(x, y)].bg,
                                Color::Reset,
                                "{} on {state} at {width}x{height}: cell ({x}, {y}) \
                                 painted over the terminal's own background",
                                theme.name()
                            );
                        }
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

    #[test]
    fn rendering_respects_the_terminals_colour_depth() {
        let mut app = ready(Screen::Weather);
        app.theme = Theme::GruvboxDark;
        app.color_depth = ColorDepth::Ansi256;
        let expected_accent = Color::Indexed(208);

        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let buffer = terminal.backend().buffer();
        let colours: Vec<Color> = (0..40u16)
            .flat_map(|y| (0..120u16).map(move |x| buffer[(x, y)].fg))
            .collect();

        assert!(colours.contains(&expected_accent));
        assert!(
            colours
                .into_iter()
                .all(|colour| !matches!(colour, Color::Rgb(..))),
            "an RGB colour escaped into indexed rendering"
        );
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

    /// The daily count is the server's word, and 65 535 of them is the exact
    /// count whose `as u16` cast lands on the edge that `+ 1` walks off.
    /// Before the clamp this overflowed the table-height arithmetic — a
    /// panic in a debug build, a degenerate layout in a release one.
    #[test]
    fn a_daily_series_the_size_of_a_u16_does_not_overflow_the_layout() {
        let mut app = ready(Screen::Weather);
        app.weather = Fetch::Ready(Weather::fixture(usize::from(u16::MAX), 0));

        drawn(&app, probe(), 80, 24);
    }

    /// `v` flips the hourly screen between its two renderings. Both must
    /// actually draw: the choice belongs to the user, not the release.
    #[test]
    fn the_hourly_screen_offers_both_views() {
        let mut app = ready(Screen::Hourly);
        let weathergram =
            symbols(&drawn(&app, Theme::default().palette(), 100, 24), 100, 24).join("\n");
        assert!(
            weathergram.contains("Hourly weather · next"),
            "default view lost the weathergram:\n{weathergram}"
        );

        app.hourly_view = HourlyView::Classic;
        let classic =
            symbols(&drawn(&app, Theme::default().palette(), 100, 24), 100, 24).join("\n");
        assert!(
            classic.contains("Precipitation · next"),
            "classic view lost its chart:\n{classic}"
        );
        assert!(
            !classic.contains("Hourly weather"),
            "classic view still drew the weathergram:\n{classic}"
        );
    }

    #[test]
    fn hourly_screen_is_usable_at_the_declared_minimum() {
        let app = ready(Screen::Hourly);
        let buffer = drawn(&app, Theme::default().palette(), MIN_WIDTH, MIN_HEIGHT);
        let text = symbols(&buffer, MIN_WIDTH, MIN_HEIGHT).join("\n");

        for label in ["sky", "temp", "rain", "wind"] {
            assert!(text.contains(label), "minimum lost {label}:\n{text}");
        }
        assert!(!text.contains("Terminal too small"));
    }

    #[test]
    fn minimum_hourly_inspector_keeps_extreme_facts_in_both_units() {
        let mut app = ready(Screen::Hourly);
        app.location.label = "A location name far longer than the minimum terminal".to_string();
        let Fetch::Ready(weather) = &mut app.weather else {
            panic!("ready weather")
        };
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now).take(24) {
            hour.chance = Some(100);
            hour.code = Some(99);
            hour.precip_mm = Some(120.5);
            hour.snow_cm = Some(0.0);
            hour.wind_kph = Some(200.0);
            hour.gust_kph = Some(300.0);
            hour.wind_dir_deg = Some(225.0);
        }

        for (unit, facts) in [
            (Unit::Metric, ["100%", "120.5mm", "SW200g300", "24h2892mm"]),
            (
                Unit::Imperial,
                ["100%", "4.74in", "SW124g186", "24h113.86in"],
            ),
        ] {
            app.unit = unit;
            let rows = symbols(
                &drawn(&app, Theme::default().palette(), MIN_WIDTH, MIN_HEIGHT),
                MIN_WIDTH,
                MIN_HEIGHT,
            );
            let text = rows.join("\n");
            assert!(
                text.contains("Thunderstorm, heavy hail"),
                "minimum lost its spelled-out condition in {unit:?}:\n{text}"
            );
            let detail = rows
                .iter()
                .find(|row| row.contains("100%"))
                .unwrap_or_else(|| panic!("compact detail missing in {unit:?}:\n{text}"));
            for fact in facts {
                assert!(
                    detail.contains(fact),
                    "minimum clipped {fact:?} in {unit:?}: {detail:?}"
                );
            }
            assert!(
                detail.starts_with('│') && detail.ends_with('│'),
                "compact detail overwrote its border in {unit:?}: {detail:?}"
            );
        }
    }

    #[test]
    fn hourly_height_tiers_drop_week_before_core_tracks() {
        let app = ready(Screen::Hourly);
        let short = symbols(&drawn(&app, Theme::default().palette(), 80, 12), 80, 12).join("\n");
        let tall = symbols(&drawn(&app, Theme::default().palette(), 100, 30), 100, 30).join("\n");

        assert!(!short.contains("this week"));
        assert!(tall.contains("this week"));
        for text in [&short, &tall] {
            for label in ["sky", "temp", "rain", "wind"] {
                assert!(text.contains(label), "lost {label}:\n{text}");
            }
        }
    }

    /// The notice is one muted line above the key bar — in the label role,
    /// where information that is not a reading already lives — and centred
    /// like the bar, so the two read as one block rather than a centred row
    /// over a flush-left one.
    #[test]
    fn the_update_notice_sits_centred_above_the_key_bar_in_muted() {
        let (width, height) = (100, 20);
        let buffer = drawn(&noticed(Screen::Weather), probe(), width, height);
        let rows = symbols(&buffer, width, height);

        let row = rows
            .iter()
            .position(|row| row.contains("update: virga 9.9.9"))
            .expect("the notice was not drawn");

        assert!(
            row >= height as usize - 3,
            "row {row} of {height} is not just above the key bar"
        );

        let text = &rows[row];
        let leading = text.chars().take_while(|c| *c == ' ').count();
        let trailing = text.chars().rev().take_while(|c| *c == ' ').count();
        assert!(
            leading > 0,
            "the notice is pinned to the left edge: {text:?}"
        );
        assert!(
            leading.abs_diff(trailing) <= 1,
            "{leading} blank columns left, {trailing} right: {text:?}"
        );

        assert_eq!(
            buffer[(leading as u16, row as u16)].style().fg,
            Some(probe().muted),
            "the notice is not in the muted role"
        );
    }

    /// At the minimum height every row is already spoken for; news must not
    /// cost the forecast a line.
    #[test]
    fn the_minimum_terminal_keeps_every_row_for_the_weather() {
        let buffer = drawn(&noticed(Screen::Weather), probe(), MIN_WIDTH, MIN_HEIGHT);
        let text = symbols(&buffer, MIN_WIDTH, MIN_HEIGHT).join("\n");

        assert!(
            !text.contains("update:"),
            "the notice took a row it was not given"
        );
        assert!(text.contains("feels like"), "the weather still renders");
    }

    /// Search lays itself out over the whole area, and news can wait for the
    /// choosing to finish.
    #[test]
    fn the_search_screen_is_left_alone_by_the_notice() {
        let buffer = drawn(&noticed(Screen::Search), probe(), 100, 20);
        let text = symbols(&buffer, 100, 20).join("\n");

        assert!(!text.contains("update: virga"));
    }

    /// The complaint this replaced a role for: the bars in both charts were a
    /// colour of their own, so the hero digits and the columns below them read
    /// as two unrelated things rather than one reading at two sizes. They share
    /// `accent` now, and the way to keep them sharing it is to assert that
    /// nothing on either chart reaches for a fourth colour.
    #[test]
    fn the_bars_are_drawn_in_the_same_colour_as_the_hero_digits() {
        // Every glyph either chart draws a column out of, and the ones the big
        // digits are built from — they overlap, which is the point.
        const BLOCKS: [&str; 12] = [
            "\u{2588}", "\u{2587}", "\u{2586}", "\u{2585}", "\u{2584}", "\u{2583}", "\u{2582}",
            "\u{2581}", "\u{2580}", "\u{2594}", "\u{258c}", "\u{2590}",
        ];

        let probe = probe();
        let allowed = [probe.accent, probe.selection, probe.now];

        for screen in [Screen::Weather, Screen::Hourly] {
            let app = ready(screen);
            let buffer = drawn(&app, probe, 120, 40);
            let mut saw_accent = false;

            for y in 0..40u16 {
                for x in 0..120u16 {
                    let cell = &buffer[(x, y)];
                    if !BLOCKS.contains(&cell.symbol()) {
                        continue;
                    }
                    assert!(
                        allowed.contains(&cell.fg),
                        "{screen:?}: the block at ({x}, {y}) is {:?}, which is neither \
                         the selection, nor now, nor the colour the hero digits use",
                        cell.fg
                    );
                    saw_accent |= cell.fg == probe.accent;
                }
            }

            assert!(
                saw_accent,
                "{screen:?} drew no ordinary blocks at all, so this proved nothing"
            );
        }
    }
}
