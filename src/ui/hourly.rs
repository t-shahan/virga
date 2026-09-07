//! The hourly forecast screen: a selected-hour inspector above a weathergram.
//! The inspector's frame, titles and precipitation summaries are shared with
//! the classic view in `inspector.rs`; this file owns what is particular to
//! the weathergram — a temperature hero, the readings beside it, and how the
//! panels stack.

use crate::app::App;
use crate::theme::Palette;
use crate::ui::inspector::{self, Hero, amount_line, inspector_render, total_line, window_from};
use crate::ui::pane::{compass, detail_line};
use crate::ui::precip_week::precip_week_render;
use crate::ui::weathergram::weathergram_render;
use crate::ui::{UNKNOWN, precip_week, weathergram};
use crate::units::Unit;
use crate::weather::model::HourlyForecast;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;

const FULL_PAIR_ROWS: u16 = inspector::ROWS + weathergram::FULL_ROWS;
const PANEL_GAP_ROWS: u16 = 1;

/// The least this screen draws in, borders included. There is no smaller
/// layout. The compact one that used to take over below this was a
/// prototype's leftover that nobody could read (#50), and the caller says
/// what the screen needs instead. `MIN_ROWS` is content rows, not terminal
/// rows; the key bar under the content is the caller's to count.
pub(super) const MIN_ROWS: u16 = FULL_PAIR_ROWS;
pub(super) const MIN_WIDTH: u16 = inspector::WIDTH;

pub(super) fn fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_ROWS
}

pub(super) fn hourly_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect) {
    let crate::app::Fetch::Ready(weather) = &app.weather else {
        return;
    };

    // Nothing here has a rendering below the floor; the caller checks it and
    // draws the size message in this area instead.
    if !fits(area) {
        return;
    }

    let hours = weather.forecast_hours();
    let selected = app.selected_hour;
    let hour = hours.get(selected);

    let spare_rows = area.height.saturating_sub(FULL_PAIR_ROWS);
    let pair_gap = if spare_rows >= PANEL_GAP_ROWS {
        PANEL_GAP_ROWS
    } else {
        0
    };
    let pair_rows = inspector::ROWS + pair_gap + weathergram::FULL_ROWS;
    let week = inspector::week_strip(hours, area, pair_rows + PANEL_GAP_ROWS);
    let week_gap = if week.is_some() { PANEL_GAP_ROWS } else { 0 };
    let week_box = week
        .as_ref()
        .map_or(0, |(_, rows)| precip_week::box_rows(*rows));

    // Whatever height the panels decline splits evenly around the stack, the
    // same move the canvas already makes with surplus width. The stack is
    // content-sized on purpose; margins that frame it read as composition,
    // while the same rows pooled under the last panel read as a screen that
    // ran out of things to say. The odd row goes below, where a floor
    // carries more weight than a ceiling.
    let used_rows = pair_rows + week_gap + week_box;
    let top_margin = area.height.saturating_sub(used_rows) / 2;

    let [
        _top_margin,
        inspector_area,
        _pair_gap,
        gram,
        _week_gap,
        week_area,
        _bottom_margin,
    ] = Layout::vertical([
        Constraint::Length(top_margin),
        Constraint::Length(inspector::ROWS),
        Constraint::Length(pair_gap),
        Constraint::Length(weathergram::FULL_ROWS),
        Constraint::Length(week_gap),
        Constraint::Length(week_box),
        Constraint::Fill(1),
    ])
    .areas(area);

    inspector_render(
        frame,
        app,
        hours,
        palette,
        inspector_area,
        &hero_temperature(hour, app.unit),
        detail_lines(hours, selected, palette, app.unit),
    );
    weathergram_render(frame, hours, palette, gram, app.unit, selected);
    if let Some((days, _)) = &week {
        precip_week_render(frame, days, palette, week_area, app.unit, selected);
    }
}

/// Temperature is the hero because it is what the weathergram's tallest
/// track draws, and the one number people want first.
fn hero_temperature(hour: Option<&HourlyForecast>, unit: Unit) -> Hero<'static> {
    let value = hour.and_then(|h| h.temp_c).map_or_else(
        || "--".to_string(),
        |temp| format!("{:.0}", unit.temp_rounded(temp)),
    );
    Hero {
        value,
        suffix: unit.temp_symbol(),
    }
}

fn detail_lines(
    hours: &[HourlyForecast],
    selected: usize,
    palette: Palette,
    unit: Unit,
) -> Vec<Line<'static>> {
    let hour = hours.get(selected);
    let ahead = window_from(hours, selected);

    vec![
        detail_line("feels like", &feels_line(hour, unit), palette),
        detail_line("humidity", &humidity_line(hour), palette),
        detail_line("precip", &precip_line(hour, unit), palette),
        detail_line("wind", &wind_line(hour, unit), palette),
        detail_line("24 h total", &total_line(ahead, unit), palette),
    ]
}

fn feels_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    hour.and_then(|h| h.feels_like_c).map_or_else(
        || UNKNOWN.to_string(),
        |c| format!("{:.0}{}", unit.temp_rounded(c), unit.temp_symbol()),
    )
}

fn humidity_line(hour: Option<&HourlyForecast>) -> String {
    hour.and_then(|h| h.humidity_pct)
        .map_or_else(|| UNKNOWN.to_string(), |value| format!("{value}%"))
}

fn precip_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    let chance = hour
        .and_then(|h| h.chance)
        .map_or_else(|| "—".to_string(), |value| format!("{value}%"));
    let amount = match amount_line(hour, unit) {
        amount if amount == UNKNOWN => "—".to_string(),
        amount => amount,
    };
    format!("{chance} · {amount}")
}

fn wind_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    let Some(hour) = hour else {
        return UNKNOWN.to_string();
    };
    let speed = hour.wind_kph.map_or_else(
        || "—".to_string(),
        |speed| format!("{:.0}", unit.speed(speed)),
    );
    let gust = hour
        .gust_kph
        .map(|gust| format!(", gusts {:.0}", unit.speed(gust)))
        .unwrap_or_default();
    let direction = hour
        .wind_dir_deg
        .map(compass)
        .map_or(String::new(), |direction| format!(" {direction}"));
    format!("{speed}{gust} {}{direction}", unit.speed_label())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActiveLocation, Fetch, Screen};
    use crate::theme::Theme;
    use crate::ui::digits::{DIGIT_ROWS, big_digits};
    use crate::ui::inspector::{DETAIL_WIDTH, dry_hours, evening_hours};
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn palette() -> Palette {
        Theme::default().palette()
    }

    const CITY: &str = "Frederick, Maryland, United States";

    fn app_showing(hours: Vec<HourlyForecast>, selected: usize) -> App {
        let mut weather = Weather::fixture(22, 14);
        weather.hourly = hours;
        weather.now_hour = 0;

        let mut app = App::new();
        app.screen = Screen::Hourly;
        app.location = ActiveLocation {
            label: CITY.to_string(),
            ..Default::default()
        };
        app.weather = Fetch::Ready(weather);
        app.selected_hour = selected;
        app
    }

    /// The rows the strip draws are calendar dates, so the rows the layout
    /// reserves have to be counted the same way.
    ///
    /// They were not: the layout divided the hour count by 24, which is the
    /// number of *days* a window spans and not the number of dates it touches.
    /// A window opening at 6 PM touches one more date than that, so the last
    /// one was dropped even with the height to draw it — and since the arrows
    /// still reached those hours, selecting one left the strip with no marked
    /// row at all.
    #[test]
    fn the_week_strip_keeps_a_last_partial_day_it_has_room_for() {
        // 60 hours from 6 PM: six hours today, two whole days, six hours of a
        // fourth date. Hour 57 is on that fourth date.
        let app = app_showing(evening_hours(60), 57);
        let text = rendered(100, 29, &app);

        let rows: Vec<&str> = text
            .lines()
            .skip_while(|line| !line.contains("this week"))
            // The title border, then the hour axis.
            .skip(2)
            .take_while(|line| !line.contains('└'))
            .collect();

        assert_eq!(
            rows.len(),
            4,
            "60 hours from 6 PM is four dates, not {}:\n{text}",
            rows.len()
        );
        assert!(rows[0].contains("Today"), "{:?}", rows[0]);
        assert!(
            rows[3].contains('▸'),
            "the selection is on the fourth date but no row is marked:\n{text}"
        );
    }

    /// It describes the world, not the highlight, so it must not move when the
    /// selection does.
    #[test]
    fn the_next_wet_hour_does_not_follow_the_selection() {
        let mut hours = dry_hours(96);
        hours[3].precip_mm = Some(0.4);

        let expected = "next rain in 3 h";
        for selected in [0usize, 3, 20, 95] {
            let app = app_showing(hours.clone(), selected);
            let text = rendered(100, 24, &app);
            assert!(
                text.contains(expected),
                "selection {selected} moved the next rain:\n{text}"
            );
        }
    }

    #[test]
    fn selected_hour_detail_helpers_keep_independent_missing_values() {
        let mut hours = dry_hours(1);
        let hour = hours.first();
        assert_eq!(feels_line(hour, Unit::Imperial), "66°F");
        assert_eq!(humidity_line(hour), "55%");
        assert_eq!(precip_line(hour, Unit::Metric), "10% · none expected");
        assert_eq!(wind_line(hour, Unit::Metric), "10, gusts 18 km/h SW");

        hours[0].wind_dir_deg = None;
        assert_eq!(wind_line(hours.first(), Unit::Metric), "10, gusts 18 km/h");
        hours[0].wind_kph = None;
        assert_eq!(wind_line(hours.first(), Unit::Metric), "—, gusts 18 km/h");
    }

    #[test]
    fn precipitation_probability_and_amount_render_independently() {
        let mut hours = dry_hours(1);
        hours[0].chance = None;
        hours[0].precip_mm = Some(1.2);
        assert_eq!(precip_line(hours.first(), Unit::Metric), "— · 1.2 mm");

        hours[0].snow_cm = Some(2.5);
        assert_eq!(precip_line(hours.first(), Unit::Metric), "— · 2.5 cm snow");

        hours[0].chance = Some(80);
        hours[0].precip_mm = None;
        hours[0].snow_cm = None;
        assert_eq!(precip_line(hours.first(), Unit::Metric), "80% · —");
    }

    #[test]
    fn missing_sustained_wind_keeps_reported_gust_and_direction() {
        let mut hours = dry_hours(1);
        hours[0].wind_kph = None;

        assert_eq!(
            wind_line(hours.first(), Unit::Metric),
            "—, gusts 18 km/h SW"
        );
    }

    #[test]
    fn full_total_is_unavailable_when_every_measurement_is_missing() {
        let mut hours = dry_hours(24);
        for hour in &mut hours {
            hour.precip_mm = None;
        }

        let app = app_showing(hours, 0);
        let text = rendered(100, FULL_PAIR_ROWS, &app);
        let total = text
            .lines()
            .find(|line| line.contains("24 h total"))
            .expect("full total line");
        assert!(
            total.contains(UNKNOWN),
            "all-missing total claimed dryness: {total:?}"
        );
    }

    /// Every value has to fit beside its label or it is clipped mid-word, and
    /// metric is the system that overflows first — a real bug the daily pane
    /// only caught because its test covered both.
    #[test]
    fn detail_values_fit_the_column() {
        let room = (DETAIL_WIDTH - 12) as usize;
        let mut hours = dry_hours(48);

        // A tropical-storm hour, and a blizzard hour, in the same series.
        for hour in hours.iter_mut() {
            hour.precip_mm = Some(120.5);
            hour.chance = Some(100);
            hour.temp_c = Some(-40.0);
        }
        hours[11].snow_cm = Some(45.7);

        for unit in [Unit::Metric, Unit::Imperial] {
            for selected in [0usize, 11, 47] {
                for line in detail_lines(&hours, selected, palette(), unit) {
                    let width = line.width();
                    assert!(
                        width <= room + 12,
                        "{unit:?} hour {selected}: {:?} is {width} wide, column allows {}",
                        line.to_string(),
                        room + 12
                    );
                }
            }
        }
    }

    fn rendered(width: u16, height: u16, app: &App) -> String {
        let buffer = rendered_buffer(width, height, app);
        buffer_text(&buffer, width, height)
    }

    fn buffer_text(buffer: &Buffer, width: u16, height: u16) -> String {
        (0..height)
            .map(|y| row_text(buffer, width, y))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn row_text(buffer: &Buffer, width: u16, y: u16) -> String {
        (0..width).map(|x| buffer[(x, y)].symbol()).collect()
    }

    fn rendered_buffer(width: u16, height: u16, app: &App) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| hourly_render(f, app, palette(), f.area()))
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn marker_coordinates(
        buffer: &Buffer,
        width: u16,
        height: u16,
        marker: &str,
    ) -> Vec<(u16, u16)> {
        (0..height)
            .flat_map(|y| {
                (0..width)
                    .filter(move |x| buffer[(*x, y)].symbol() == marker)
                    .map(move |x| (x, y))
            })
            .collect()
    }

    fn track_row(buffer: &Buffer, width: u16, height: u16, label: &str) -> u16 {
        (0..height)
            .find(|y| {
                let line = row_text(buffer, width, *y);
                line.trim_start_matches([' ', '│']).starts_with(label)
            })
            .unwrap_or_else(|| panic!("{label} track missing"))
    }

    fn track_text(buffer: &Buffer, width: u16, height: u16, label: &str) -> String {
        let y = track_row(buffer, width, height, label);
        row_text(buffer, width, y)
    }

    fn track_glyph_coordinates(
        buffer: &Buffer,
        width: u16,
        height: u16,
        label: &str,
        glyphs: &[&str],
    ) -> Vec<(u16, String)> {
        let y = track_row(buffer, width, height, label);

        (1..width.saturating_sub(1))
            .filter_map(|x| {
                let symbol = buffer[(x, y)].symbol();
                glyphs.contains(&symbol).then(|| (x, symbol.to_string()))
            })
            .collect()
    }

    /// Unit conversion may change a number or its suffix, but never the chart
    /// cell it belongs to. These are the six widths either side of every
    /// measured horizon boundary (24, 36, and 48 hours).
    #[test]
    fn units_never_move_hourly_weathergram_symbols() {
        for width in [68, 69, 92, 93, 116, 117] {
            let mut metric = app_showing(dry_hours(192), 24);
            metric.unit = Unit::Metric;
            let mut imperial = app_showing(dry_hours(192), 24);
            imperial.unit = Unit::Imperial;

            let metric = weathergram_symbol_positions(&rendered_buffer(width, 24, &metric), width);
            let imperial =
                weathergram_symbol_positions(&rendered_buffer(width, 24, &imperial), width);
            assert_eq!(
                metric, imperial,
                "unit conversion moved a weathergram symbol at {width} columns"
            );
        }
    }

    fn weathergram_symbol_positions(buffer: &Buffer, width: u16) -> Vec<(u16, u16, String)> {
        (inspector::ROWS..FULL_PAIR_ROWS)
            .flat_map(|y| {
                (0..width).filter_map(move |x| {
                    let symbol = buffer[(x, y)].symbol();
                    ((!symbol.is_ascii() && !matches!(symbol, "°" | "–"))
                        || matches!(symbol, "*" | "?"))
                    .then(|| (x, y, symbol.to_string()))
                })
            })
            .collect()
    }

    #[test]
    fn full_hero_rounds_converted_temperature_and_marks_missing_values() {
        let mut hours = dry_hours(1);
        hours[0].temp_c = Some(0.4);
        let hero = hero_temperature(hours.first(), Unit::Imperial);
        assert_eq!(hero.value, "33");
        assert_eq!(hero.suffix, "°F");

        let missing = hero_temperature(None, Unit::Metric);
        assert_eq!(missing.value, "--");
        assert_eq!(missing.suffix, "°C");

        // Just below zero rounds to zero, not to a signed zero the block
        // font would draw as a dash and a nought.
        let mut below_zero = dry_hours(1);
        below_zero[0].temp_c = Some(-0.3);
        assert_eq!(
            hero_temperature(below_zero.first(), Unit::Metric).value,
            "0"
        );

        // And on screen: the digits, then the symbol on the middle row.
        let app = app_showing(hours, 0);
        let text = rendered(100, 24, &app);
        let expected = big_digits("33");
        for row in &expected {
            assert!(text.contains(row.trim_end()), "{row:?} missing:\n{text}");
        }
        let symbol_row = text.lines().nth(1 + DIGIT_ROWS / 2).unwrap_or_default();
        assert!(symbol_row.contains("°F"), "{symbol_row:?}");
    }

    /// The border used to carry the chance beside the condition, which made
    /// three copies of one number.
    #[test]
    fn the_border_carries_the_condition_and_not_the_chance() {
        let mut hours = dry_hours(48);
        for hour in hours.iter_mut() {
            hour.chance = Some(53);
            hour.code = Some(61);
        }
        let app = app_showing(hours, 0);
        let text = rendered(100, 24, &app);
        let top = text
            .lines()
            .find(|line| line.contains('┌'))
            .expect("a top border");

        assert!(
            top.contains("rain"),
            "the condition rides the border: {top:?}"
        );
        assert!(!top.contains('%'), "the chance does not: {top:?}");
    }

    #[test]
    fn full_inspector_states_every_selected_hour_fact() {
        let app = app_showing(dry_hours(192), 3);
        let text = rendered(100, 24, &app);

        for label in ["feels like", "humidity", "precip", "wind", "24 h total"] {
            assert!(text.contains(label), "lost {label:?}:\n{text}");
        }
        assert!(text.contains("Hourly"), "weathergram missing:\n{text}");
    }

    #[test]
    fn tall_layout_retains_the_weekly_precipitation_strip() {
        let app = app_showing(dry_hours(192), 0);
        let text = rendered(100, 30, &app);
        assert!(text.contains("Hourly"), "weathergram missing:\n{text}");
        assert!(text.contains("this week"), "weekly strip missing:\n{text}");
    }

    /// Surplus height splits evenly around the stack, so a tall terminal
    /// frames the panels instead of pooling every spare row under the last
    /// one. An exactly-fitting height still spends every row on content.
    #[test]
    fn surplus_rows_frame_the_stack_symmetrically() {
        let app = app_showing(dry_hours(192), 0);

        // 36 rows: the full pair and an eight-day strip use 32, and the four
        // spare rows split two above, two below.
        let buffer = rendered_buffer(100, 36, &app);
        let panel_tops: Vec<u16> = (0..36)
            .filter(|y| buffer[(0, *y)].symbol() == "┌")
            .collect();
        assert_eq!(panel_tops, [2, 10, 23]);
        for y in [0, 1, 9, 22, 34, 35] {
            assert!(
                row_text(&buffer, 100, y).trim().is_empty(),
                "row {y} should be margin or gap"
            );
        }

        // 30 rows fit the stack exactly: no margin above, none below, and the
        // week strip takes the rows the margins would otherwise get.
        let exact = rendered_buffer(100, 30, &app);
        let tops: Vec<u16> = (0..30).filter(|y| exact[(0, *y)].symbol() == "┌").collect();
        assert_eq!(tops, [0, 8, 21]);
    }

    /// One spare row cannot provide both outer padding and a panel gap. The
    /// gap wins because it separates two distinct frames instead of leaving
    /// an unexplained blank below them.
    #[test]
    fn one_spare_row_separates_the_inspector_and_weathergram() {
        let app = app_showing(dry_hours(192), 0);
        let height = FULL_PAIR_ROWS + 1;
        let buffer = rendered_buffer(100, height, &app);
        let panel_tops: Vec<u16> = (0..height)
            .filter(|y| buffer[(0, *y)].symbol() == "┌")
            .collect();

        assert_eq!(panel_tops, [0, inspector::ROWS + 1]);
        assert!(
            row_text(&buffer, 100, inspector::ROWS).trim().is_empty(),
            "the only spare row should separate panels"
        );
    }

    /// Titles replace border cells, so a literal space is the only reliable
    /// inset across terminal emulators and box-drawing fonts.
    #[test]
    fn full_panel_titles_keep_one_cell_clear_of_their_corners() {
        let app = app_showing(dry_hours(192), 0);
        let buffer = rendered_buffer(100, 30, &app);

        // The 30-row layout fits exactly, so the panel borders sit at rows
        // 0, 8, and 21; the cell beside each corner is the title's inset.
        assert_eq!(buffer[(0, 0)].symbol(), "┌", "inspector border moved");
        assert_eq!(buffer[(1, 0)].symbol(), " ", "inspector title inset");
        assert_eq!(buffer[(0, 8)].symbol(), "┌", "weathergram border moved");
        assert_eq!(buffer[(1, 8)].symbol(), " ", "weathergram title inset");
        assert_eq!(buffer[(0, 21)].symbol(), "┌", "weekly border moved");
        assert_eq!(buffer[(98, 21)].symbol(), " ", "weekly title inset");
    }

    /// Rendering the full state matrix proves the selected-hour inspector and
    /// weathergram degrade together: an empty forecast, missing readings, and
    /// every navigation edge still leave the four tracks available.
    #[test]
    fn hourly_layout_keeps_tracks_through_boundary_forecasts() {
        let mut unavailable = dry_hours(192);
        for hour in &mut unavailable {
            hour.precip_mm = None;
            hour.snow_cm = None;
            hour.chance = None;
            hour.code = None;
            hour.temp_c = None;
            hour.feels_like_c = None;
            hour.humidity_pct = None;
            hour.wind_kph = None;
            hour.gust_kph = None;
            hour.wind_dir_deg = None;
        }

        let mut boundary_values = dry_hours(192);
        for hour in &mut boundary_values {
            hour.temp_c = Some(-5.0);
            hour.chance = None;
            hour.wind_dir_deg = None;
        }
        for (hour, chance) in boundary_values
            .iter_mut()
            .zip([9, 10, 29, 30, 49, 50, 69, 70])
        {
            hour.chance = Some(chance);
        }
        for (hour, direction) in boundary_values.iter_mut().zip([-45.0, 0.0, 337.5, 360.0]) {
            hour.wind_dir_deg = Some(direction);
        }

        let cases = [
            ("empty", dry_hours(0), 0),
            ("one hour", dry_hours(1), 0),
            ("first hour", dry_hours(192), 0),
            ("page edge", dry_hours(192), 24),
            ("last hour", dry_hours(192), 191),
            ("all optional readings missing", unavailable, 0),
            (
                "flat below-zero temperatures and threshold readings",
                boundary_values,
                0,
            ),
        ];

        for (case, hours, selected) in cases {
            let mut app = app_showing(hours, selected);
            if case == "flat below-zero temperatures and threshold readings" {
                app.unit = Unit::Metric;
            }
            for (width, height) in [
                (MIN_WIDTH, MIN_ROWS),
                (80, 24),
                (100, 24),
                (120, 30),
                (200, 50),
            ] {
                let buffer = rendered_buffer(width, height, &app);
                let text = buffer_text(&buffer, width, height);
                for label in ["sky", "temp", "rain", "wind"] {
                    assert!(
                        text.contains(label),
                        "{case} at {width}x{height} lost {label}:\n{text}"
                    );
                }

                if (width, height) != (80, 24) {
                    continue;
                }

                match case {
                    "empty" => {
                        assert_eq!(marker_coordinates(&buffer, width, height, "▲"), []);
                        assert!(
                            marker_coordinates(&buffer, width, height, "┬").is_empty(),
                            "the empty chart drew a redundant current marker:\n{text}"
                        );
                    }
                    "one hour" => assert_eq!(
                        marker_coordinates(&buffer, width, height, "▲"),
                        [(36, 20)],
                        "the single hour is not centered in its visible cell:\n{text}"
                    ),
                    "first hour" => assert_eq!(
                        marker_coordinates(&buffer, width, height, "▲"),
                        [(12, 20)],
                        "{case} selection is not on the first visible hour:\n{text}"
                    ),
                    "page edge" => assert_eq!(
                        marker_coordinates(&buffer, width, height, "▲"),
                        [(12, 20)],
                        "the next page did not begin at its selected hour:\n{text}"
                    ),
                    "last hour" => assert_eq!(
                        marker_coordinates(&buffer, width, height, "▲"),
                        [(58, 20)],
                        "the final page did not keep the last hour selected:\n{text}"
                    ),
                    "all optional readings missing" => {
                        for label in ["temp", "rain", "wind"] {
                            assert!(
                                track_text(&buffer, width, height, label).contains('—'),
                                "{label} treated missing readings as data:\n{text}"
                            );
                        }
                        assert!(
                            track_glyph_coordinates(
                                &buffer,
                                width,
                                height,
                                "sky",
                                &[
                                    "○",
                                    "⊙",
                                    "●",
                                    "≡",
                                    "┆",
                                    "│",
                                    "*",
                                    "ϟ",
                                    "?",
                                    "☀\u{fe0f}",
                                    "⛅",
                                    "☁\u{fe0f}",
                                    "🌫\u{fe0f}",
                                    "🌦\u{fe0f}",
                                    "🌧\u{fe0f}",
                                    "❄\u{fe0f}",
                                    "⛈\u{fe0f}",
                                    "❓"
                                ]
                            )
                            .is_empty(),
                            "missing conditions drew a weather symbol:\n{text}"
                        );
                    }
                    "flat below-zero temperatures and threshold readings" => {
                        assert!(
                            track_text(&buffer, width, height, "temp").contains("-5–-5°C"),
                            "flat below-zero range changed meaning:\n{text}"
                        );
                        let flat = track_glyph_coordinates(
                            &buffer,
                            width,
                            height,
                            "temp",
                            &["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"],
                        );
                        assert_eq!(
                            flat.len(),
                            48,
                            "a flat window should ink every plot cell:\n{text}"
                        );
                        assert!(
                            flat.iter().all(|(_, glyph)| glyph == "█"),
                            "a flat window should stand at its half-height rows:\n{text}"
                        );
                        assert_eq!(flat.first(), Some(&(12, "█".to_string())));
                        assert_eq!(flat.last(), Some(&(59, "█".to_string())));
                        assert_eq!(
                            track_glyph_coordinates(
                                &buffer,
                                width,
                                height,
                                "rain",
                                &["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"]
                            ),
                            vec![
                                (14, "▂".to_string()),
                                (15, "▂".to_string()),
                                (16, "▅".to_string()),
                                (17, "▅".to_string()),
                                (18, "▅".to_string()),
                                (19, "▅".to_string()),
                                (20, "█".to_string()),
                                (21, "█".to_string()),
                                (22, "█".to_string()),
                                (23, "█".to_string()),
                                (24, "█".to_string()),
                                (25, "█".to_string()),
                                (26, "█".to_string()),
                                (27, "█".to_string()),
                            ],
                            "rain heights no longer render their visible band:\n{text}"
                        );
                        assert_eq!(
                            track_glyph_coordinates(
                                &buffer,
                                width,
                                height,
                                "wind",
                                &["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"]
                            ),
                            vec![(12, "↖".to_string()), (16, "↑".to_string())],
                            "wind sectors no longer render on their cadence:\n{text}"
                        );
                    }
                    _ => unreachable!("unknown boundary case: {case}"),
                }
            }
        }
    }

    #[test]
    fn minimum_width_full_height_keeps_every_wind_fact() {
        let mut hours = dry_hours(24);
        hours[0].wind_kph = Some(200.0);
        hours[0].gust_kph = Some(300.0);
        hours[0].wind_dir_deg = Some(225.0);
        let mut app = app_showing(hours, 0);
        app.unit = Unit::Metric;

        let text = rendered(MIN_WIDTH, MIN_ROWS, &app);
        assert!(text.contains("200"), "wind speed was lost:\n{text}");
        assert!(text.contains("300"), "wind gust was lost:\n{text}");
        assert!(text.contains("SW"), "wind direction was lost:\n{text}");
    }

    /// Below the floor the caller draws the size message over this area, so
    /// anything drawn here would sit under it. A partial layout at these
    /// sizes is what the compact one used to be.
    #[test]
    fn below_the_floor_nothing_is_drawn() {
        let app = app_showing(dry_hours(192), 0);
        for (width, height) in [(MIN_WIDTH - 1, 30), (100, MIN_ROWS - 1), (34, 12)] {
            let text = rendered(width, height, &app);
            assert!(
                text.trim().is_empty(),
                "{width}x{height} drew below the floor:\n{text}"
            );
        }
    }

    /// The awkward sizes, including the app's declared minimum and either side
    /// of the height where the chart stops fitting.
    #[test]
    fn renders_without_panicking_at_awkward_sizes() {
        for hours in [dry_hours(0), dry_hours(1), dry_hours(192)] {
            for selected in [0usize, 1, 191] {
                let app = app_showing(hours.clone(), selected);
                for (width, height) in [
                    (34, 12),
                    (34, 11),
                    (40, 7),
                    (60, 12),
                    (80, 13),
                    (100, 24),
                    (200, 50),
                    (1, 1),
                ] {
                    let _ = rendered(width, height, &app);
                }
            }
        }
    }

    #[test]
    fn the_selected_hour_is_named_on_the_border() {
        let app = app_showing(dry_hours(48), 14);
        let text = rendered(100, 24, &app);

        assert!(
            text.contains("2:00 PM"),
            "hour 14 of 10 Aug is 2 PM:\n{text}"
        );
    }
}
