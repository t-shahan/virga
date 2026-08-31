//! The hourly forecast screen: a selected-hour inspector above a weathergram.
//! A sibling of `current.rs` rather than a generalisation of it — the
//! daily pane's border budget, period comparison and hero source are all
//! specific to it, and merging the two would tangle both.

use crate::app::App;
use crate::theme::Palette;
use crate::ui::digits::{CELL_WIDTH, DIGIT_ROWS, big_digits};
use crate::ui::precip_week::precip_week_render;
use crate::ui::precipitation::{PrecipitationAggregate, aggregate};
use crate::ui::weathergram::weathergram_render;
use crate::ui::{TITLE_GUTTER, UNKNOWN, title_room, truncate};
use crate::ui::{precip_week, weathergram};
use crate::units::Unit;
use crate::weather::code::description;
use crate::weather::model::HourlyForecast;
use chrono::NaiveDateTime;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

/// Rows the full inspector occupies, borders included — the digit block plus the
/// two border rows, so it is sized from the font rather than guessed at.
const FULL_INSPECTOR_ROWS: u16 = DIGIT_ROWS as u16 + 2;
const FULL_PAIR_ROWS: u16 = FULL_INSPECTOR_ROWS + weathergram::FULL_ROWS;
const PANEL_GAP_ROWS: u16 = 1;

/// Three temperature characters (including a possible minus sign), plus the
/// two-character unit symbol.
const HERO_WIDTH: u16 = 3 * CELL_WIDTH + 2;
/// Matches the daily pane's column. A snow-flagged metric total is the widest
/// value here and needs every one of these; at 30 it clipped mid-word.
const DETAIL_WIDTH: u16 = 34;
/// Two border columns around the widest full detail line.
const FULL_INSPECTOR_WIDTH: u16 = DETAIL_WIDTH + 2;
const COLUMN_GUTTER: u16 = 3;

/// The least this screen draws in, borders included. There is no smaller
/// layout. The compact one that used to take over below this was a
/// prototype's leftover that nobody could read (#50), and the caller says
/// what the screen needs instead. `MIN_ROWS` is content rows, not terminal
/// rows; the key bar under the content is the caller's to count.
pub(super) const MIN_ROWS: u16 = FULL_PAIR_ROWS;
pub(super) const MIN_WIDTH: u16 = FULL_INSPECTOR_WIDTH;

pub(super) fn fits(area: Rect) -> bool {
    area.width >= MIN_WIDTH && area.height >= MIN_ROWS
}

/// How far ahead the summary rows look. A day is the horizon people actually
/// plan against, and it matches the vertical arrows' jump.
const SUMMARY_HOURS: usize = 24;
/// Past this an hour count stops being useful and a date reads better.
const HOURS_BEFORE_A_DATE_READS_BETTER: usize = 24;

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
    let pair_rows = FULL_INSPECTOR_ROWS + pair_gap + weathergram::FULL_ROWS;
    let week = week_strip(hours, area, pair_rows + PANEL_GAP_ROWS);
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
        inspector,
        _pair_gap,
        gram,
        _week_gap,
        week_area,
        _bottom_margin,
    ] = Layout::vertical([
        Constraint::Length(top_margin),
        Constraint::Length(FULL_INSPECTOR_ROWS),
        Constraint::Length(pair_gap),
        Constraint::Length(weathergram::FULL_ROWS),
        Constraint::Length(week_gap),
        Constraint::Length(week_box),
        Constraint::Fill(1),
    ])
    .areas(area);

    inspector_render(frame, app, hours, hour, palette, inspector);
    weathergram_render(frame, hours, palette, gram, app.unit, selected);
    if let Some((days, _)) = &week {
        precip_week_render(frame, days, palette, week_area, app.unit, selected);
    }
}

/// The week strip's grouping and how many days it may show here, or `None`
/// where it does not fit.
///
/// It is the last thing on the screen to be given rows and the first to give
/// them up: the weathergram is what the arrows move through, and a strip that
/// squeezed it would cost more than it adds.
///
/// Grouped once, here, for the layout and the strip both. The rows this
/// reserves and the rows the grid draws used to be two separate walks over the
/// series, kept equal by a test; sharing one grouping makes them equal by
/// construction and halves what the screen's most redrawn frame pays for it.
fn week_strip(
    hours: &[HourlyForecast],
    area: Rect,
    pair_rows: u16,
) -> Option<(Vec<precip_week::Day<'_>>, usize)> {
    if area.width < precip_week::MIN_WIDTH + 2 {
        return None;
    }

    let spare = area.height.saturating_sub(pair_rows);
    let rows = spare.saturating_sub(precip_week::box_rows(0)) as usize;

    // Answered from the geometry alone, like the width check above: `rows` is
    // an upper bound on what the grouping can return, so a terminal too short
    // for the strip would pay for a grouping that is always thrown away.
    if rows < precip_week::MIN_DAYS {
        return None;
    }

    // Calendar dates, not `hours / 24`. A window opening at 6 PM is two and a
    // half days long and touches four dates, and it is dates the strip draws
    // rows for — counting days here reserved one row too few and dropped the
    // tail of the forecast even with the height to show it.
    let days = precip_week::group_by_day(hours);
    let rows = rows.min(days.len());

    (rows >= precip_week::MIN_DAYS).then_some((days, rows))
}

fn inspector_render(
    frame: &mut Frame,
    app: &App,
    hours: &[HourlyForecast],
    hour: Option<&HourlyForecast>,
    palette: Palette,
    area: Rect,
) {
    let unit = app.unit;

    let condition = hour
        .and_then(|h| h.code)
        .map_or(UNKNOWN, description)
        .to_string();
    let when = hour.map_or_else(|| "—".to_string(), |h| long_hour(&h.time));

    // Computed from now rather than from the selection: it is a fact about the
    // world, not about what is highlighted, so it should stay put while you
    // arrow around.
    let upcoming = next_precipitation(hours);

    let (city, condition) = top_titles(&app.location.label, &condition, area.width);
    let (upcoming, when) = bottom_titles(&upcoming, &when, area.width);

    let mut block = Block::bordered()
        .border_style(Style::new().fg(palette.border))
        .title_top(
            Line::from(format!(" {city}"))
                .bold()
                .fg(palette.accent)
                .left_aligned(),
        )
        .title_bottom(
            Line::from(format!("{when} "))
                .fg(palette.text)
                .right_aligned(),
        );

    if let Some(condition) = condition {
        block = block.title_top(
            Line::from(format!("{condition} "))
                .fg(palette.text)
                .right_aligned(),
        );
    }
    if let Some(upcoming) = upcoming {
        // The question people open the app to ask, so it gets the loud colour
        // and the weight rather than the one reserved for labels. It was muted
        // to begin with and read as chrome — the eye went straight past the one
        // line on the screen that answers "do I need a coat".
        block = block.title_bottom(
            Line::from(format!(" {upcoming}"))
                .bold()
                .fg(palette.selection)
                .left_aligned(),
        );
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Same rule as the daily pane: the digits are decoration and the readings
    // are the content, so drop the hero rather than clip it mid-glyph.
    let full = HERO_WIDTH + COLUMN_GUTTER + DETAIL_WIDTH;
    let show_hero = inner.width >= full;
    let wanted = if show_hero {
        full
    } else {
        DETAIL_WIDTH.min(inner.width)
    };

    let [content] = Layout::horizontal([Constraint::Length(wanted)])
        .flex(Flex::Center)
        .areas(inner);

    let (hero_area, detail_area) = if show_hero {
        let [hero, _gutter, detail] = Layout::horizontal([
            Constraint::Length(HERO_WIDTH),
            Constraint::Length(COLUMN_GUTTER),
            Constraint::Length(DETAIL_WIDTH),
        ])
        .areas(content);
        (Some(hero), detail)
    } else {
        (None, content)
    };

    if let Some(hero_area) = hero_area {
        frame.render_widget(
            Paragraph::new(hero_temperature(hour, unit, palette)).alignment(Alignment::Center),
            hero_area,
        );
    }

    frame.render_widget(
        Paragraph::new(detail_lines(hours, app.selected_hour, palette, unit)),
        detail_area,
    );
}

fn hero_temperature(
    hour: Option<&HourlyForecast>,
    unit: Unit,
    palette: Palette,
) -> Vec<Line<'static>> {
    let value = hour.and_then(|h| h.temp_c).map_or_else(
        || "--".to_string(),
        |temp| format!("{:.0}", unit.temp(temp)),
    );

    big_digits(&value)
        .iter()
        .enumerate()
        .map(|(i, row)| {
            if i == DIGIT_ROWS / 2 {
                Line::from(vec![
                    Span::from(row.clone()).bold().fg(palette.accent),
                    Span::from(unit.temp_symbol()).fg(palette.accent),
                ])
            } else {
                Line::from(format!("{row}  ")).bold().fg(palette.accent)
            }
        })
        .collect()
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
        |c| format!("{:.0}{}", unit.temp(c), unit.temp_symbol()),
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

fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    POINTS[((degrees.rem_euclid(360.0) / 45.0).round() as usize) % POINTS.len()]
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

fn window_from(hours: &[HourlyForecast], selected: usize) -> &[HourlyForecast] {
    let end = selected.saturating_add(SUMMARY_HOURS).min(hours.len());
    hours.get(selected..end).unwrap_or_default()
}

/// Snow and rain are different news, so they get different words and different
/// units — a centimetre of snow is not a millimetre of rain.
fn amount_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    let Some(hour) = hour else {
        return UNKNOWN.to_string();
    };

    if let Some(cm) = hour.snow_cm.filter(|cm| *cm > 0.0) {
        return format!(
            "{:.*} {} snow",
            unit.snow_decimals(),
            unit.snow(cm),
            unit.snow_label()
        );
    }

    match hour.precip_mm {
        Some(mm) if mm > 0.0 => measured(mm, unit),
        // "none" alone reads as a denial of the chance in the hero beside it;
        // "expected" ties it to the forecast amount, which is what is zero.
        Some(_) => "none expected".to_string(),
        None => UNKNOWN.to_string(),
    }
}

/// A positive amount that rounds to zero at the display precision must not
/// render as `0.00 in` — the forecast is not zero, it is small.
fn measured(mm: f64, unit: Unit) -> String {
    let value = unit.precip(mm);
    let decimals = unit.precip_decimals();
    let quantum = 0.1_f64.powi(decimals as i32);

    if value < quantum / 2.0 {
        return format!("<{quantum:.decimals$} {}", unit.precip_label());
    }
    format!("{value:.decimals$} {}", unit.precip_label())
}

/// Unlike a single hour, a day is very often mixed — rain turning to snow is
/// the ordinary winter case. Reporting only the snow depth would hide the rain
/// that fell with it, so the total is the precipitation total, flagged when
/// some of it arrived frozen.
fn total_line(ahead: &[HourlyForecast], unit: Unit) -> String {
    let total = aggregate(ahead, unit);
    let amount = match total {
        PrecipitationAggregate::Unavailable => return UNKNOWN.to_string(),
        PrecipitationAggregate::Zero => return "none expected".to_string(),
        PrecipitationAggregate::Trace(_) | PrecipitationAggregate::Measured(_) => total
            .positive_text(unit, " ")
            .expect("positive aggregate has text"),
    };
    if ahead.iter().any(HourlyForecast::is_snow) {
        return format!("{amount} incl. snow");
    }
    amount
}

/// The question people actually open a weather app to ask. It earns the
/// bottom-left border — the slot the daily pane spends on its period
/// comparison — and costs no interior row.
///
/// Triggered by forecast amount rather than by probability: a 20% hour with no
/// forecast accumulation is not rain, which is the confusion the daily pane
/// already had to correct once.
fn next_precipitation(hours: &[HourlyForecast]) -> String {
    let Some(now) = hours.first() else {
        return String::new();
    };

    match now.precip_mm {
        None => return "precipitation unavailable".to_string(),
        Some(_) if now.is_wet() => return format!("{}ing now", falling_word(now)),
        Some(_) => {}
    }

    for (ahead, hour) in hours.iter().enumerate().skip(1) {
        match hour.precip_mm {
            None => return "precipitation unavailable".to_string(),
            Some(_) if hour.is_wet() && ahead <= HOURS_BEFORE_A_DATE_READS_BETTER => {
                return format!("next {} in {ahead} h", falling_word(hour));
            }
            Some(_) if hour.is_wet() => {
                return format!("next {} {}", falling_word(hour), day_and_hour(&hour.time));
            }
            Some(_) => {}
        }
    }

    format!("no rain in the next {} days", hours.len() / 24)
}

fn falling_word(hour: &HourlyForecast) -> &'static str {
    if hour.is_snow() { "snow" } else { "rain" }
}

/// City left, condition right. At the minimum width the condition is the
/// actionable fact, so the location yields its columns first.
fn top_titles(name: &str, condition: &str, width: u16) -> (String, Option<String>) {
    let available = title_room(width);
    let name = name.to_uppercase();
    let len = |s: &str| s.chars().count();

    if len(&name) + TITLE_GUTTER + len(condition) <= available {
        return (name, Some(condition.to_string()));
    }
    if len(condition) <= available {
        let city_room = available.saturating_sub(len(condition) + TITLE_GUTTER);
        let city = if city_room == 0 {
            String::new()
        } else {
            truncate(&name, city_room)
        };
        return (city, Some(condition.to_string()));
    }
    (String::new(), Some(truncate(condition, available)))
}

/// The hour right, what is coming left. The hour is what changes as you arrow
/// around, so it is never the one sacrificed.
fn bottom_titles(upcoming: &str, when: &str, width: u16) -> (Option<String>, String) {
    let available = title_room(width);
    let len = |s: &str| s.chars().count();

    if !upcoming.is_empty() && len(upcoming) + TITLE_GUTTER + len(when) <= available {
        return (Some(upcoming.to_string()), when.to_string());
    }
    (None, when.to_string())
}

fn parse_hour(time: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(time, "%Y-%m-%dT%H:%M").ok()
}

fn long_hour(time: &str) -> String {
    parse_hour(time).map_or_else(
        || time.to_string(),
        |at| at.format("%a %-d %b, %-I:%M %p").to_string(),
    )
}

/// The window runs eight days, so every weekday name occurs at least once and
/// one occurs twice. A bare "Mon 3 AM" for something a week out reads as this
/// morning; the date is what makes it unambiguous.
fn day_and_hour(time: &str) -> String {
    parse_hour(time).map_or_else(
        || time.to_string(),
        |at| at.format("%a %-d %b, %-I %p").to_string(),
    )
}

fn detail_line(label: &str, value: &str, palette: Palette) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<12}")).fg(palette.muted),
        Span::from(value.to_string()).fg(palette.text),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActiveLocation, Fetch, Screen};
    use crate::theme::Theme;
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    fn palette() -> Palette {
        Theme::default().palette()
    }

    const CITY: &str = "Frederick, Maryland, United States";

    fn dry_hours(count: usize) -> Vec<HourlyForecast> {
        (0..count)
            .map(|i| HourlyForecast {
                time: format!("2026-08-{:02}T{:02}:00", 10 + i / 24, i % 24),
                precip_mm: Some(0.0),
                snow_cm: Some(0.0),
                chance: Some(10),
                code: Some(0),
                temp_c: Some(20.0),
                feels_like_c: Some(19.0),
                humidity_pct: Some(55),
                wind_kph: Some(10.0),
                gust_kph: Some(18.0),
                wind_dir_deg: Some(225.0),
            })
            .collect()
    }

    /// A window that starts in the evening, where an hour count and a count of
    /// calendar dates come apart: 60 hours from 6 PM is two and a half days
    /// long but touches four dates.
    fn evening_hours(count: usize) -> Vec<HourlyForecast> {
        dry_hours(count + 18).split_off(18)
    }

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

    #[test]
    fn rain_now_is_reported_as_now() {
        let mut hours = dry_hours(48);
        hours[0].precip_mm = Some(1.2);

        assert_eq!(next_precipitation(&hours), "raining now");
    }

    #[test]
    fn snow_gets_its_own_word() {
        let mut hours = dry_hours(48);
        hours[0].precip_mm = Some(1.2);
        hours[0].snow_cm = Some(0.9);
        assert_eq!(next_precipitation(&hours), "snowing now");

        let mut later = dry_hours(48);
        later[3].precip_mm = Some(1.2);
        later[3].snow_cm = Some(0.9);
        assert_eq!(next_precipitation(&later), "next snow in 3 h");
    }

    #[test]
    fn the_next_wet_hour_is_counted_in_hours_while_that_reads_well() {
        let mut hours = dry_hours(96);
        hours[3].precip_mm = Some(0.4);

        assert_eq!(next_precipitation(&hours), "next rain in 3 h");
    }

    /// Past a day an hour count stops meaning anything; a weekday and time is
    /// what someone can actually act on.
    #[test]
    fn a_distant_wet_hour_is_named_by_day() {
        let mut hours = dry_hours(96);
        hours[40].precip_mm = Some(0.4);

        let text = next_precipitation(&hours);
        assert!(text.starts_with("next rain "), "{text:?}");
        assert!(
            text.contains("Tue"),
            "40 h past Mon 10 Aug is Tue: {text:?}"
        );
        assert!(!text.contains(" h"), "no bare hour count that far out");
    }

    /// The common case, and the one the screen must not render as silence.
    #[test]
    fn a_dry_window_says_how_long_it_stays_dry() {
        assert_eq!(
            next_precipitation(&dry_hours(192)),
            "no rain in the next 8 days"
        );
    }

    /// Probability alone must not trigger it: an hour with a 90% chance and no
    /// forecast accumulation is not rain.
    #[test]
    fn a_high_chance_with_no_amount_is_not_rain() {
        let mut hours = dry_hours(48);
        hours[5].chance = Some(90);

        assert_eq!(next_precipitation(&hours), "no rain in the next 2 days");
    }

    #[test]
    fn an_empty_series_says_nothing_rather_than_guessing() {
        assert_eq!(next_precipitation(&[]), "");
    }

    /// It describes the world, not the highlight, so it must not move when the
    /// selection does.
    #[test]
    fn the_next_wet_hour_does_not_follow_the_selection() {
        let mut hours = dry_hours(96);
        hours[3].precip_mm = Some(0.4);
        let expected = next_precipitation(&hours);

        for selected in [0usize, 3, 20, 95] {
            let app = app_showing(hours.clone(), selected);
            let Fetch::Ready(w) = &app.weather else {
                panic!("ready")
            };
            assert_eq!(
                next_precipitation(w.forecast_hours()),
                expected,
                "selection {selected} moved it"
            );
        }
    }

    #[test]
    fn an_unknown_amount_blocks_dry_and_later_event_claims() {
        let mut later_rain = dry_hours(48);
        later_rain[1].precip_mm = None;
        later_rain[3].precip_mm = Some(0.4);
        assert_eq!(next_precipitation(&later_rain), "precipitation unavailable");

        let mut otherwise_dry = dry_hours(48);
        otherwise_dry[1].precip_mm = None;
        assert_eq!(
            next_precipitation(&otherwise_dry),
            "precipitation unavailable"
        );
    }

    /// A positive amount rounding to zero at the display precision would read
    /// as a dry hour. 0.1 mm is 0.0039 in — the case that actually occurs.
    #[test]
    fn a_trace_amount_never_renders_as_zero() {
        let mut hours = dry_hours(4);
        hours[0].precip_mm = Some(0.1);

        assert_eq!(amount_line(hours.first(), Unit::Imperial), "<0.01 in");
        assert_eq!(amount_line(hours.first(), Unit::Metric), "0.1 mm");

        hours[0].precip_mm = Some(0.01);
        assert_eq!(amount_line(hours.first(), Unit::Metric), "<0.1 mm");
    }

    #[test]
    fn a_measured_amount_renders_at_the_units_precision() {
        let mut hours = dry_hours(4);
        hours[0].precip_mm = Some(25.4);

        assert_eq!(amount_line(hours.first(), Unit::Imperial), "1.00 in");
        assert_eq!(amount_line(hours.first(), Unit::Metric), "25.4 mm");
    }

    #[test]
    fn an_exactly_dry_hour_says_none_expected() {
        let hours = dry_hours(4);
        assert_eq!(amount_line(hours.first(), Unit::Imperial), "none expected");
    }

    #[test]
    fn a_missing_amount_is_unavailable_rather_than_zero() {
        let mut hours = dry_hours(4);
        hours[0].precip_mm = None;
        hours[0].snow_cm = None;

        assert_eq!(amount_line(hours.first(), Unit::Metric), UNKNOWN);
        assert_eq!(amount_line(None, Unit::Metric), UNKNOWN);
    }

    #[test]
    fn snow_is_measured_in_its_own_units() {
        let mut hours = dry_hours(4);
        hours[0].snow_cm = Some(2.5);
        hours[0].precip_mm = Some(1.8);

        assert_eq!(amount_line(hours.first(), Unit::Metric), "2.5 cm snow");
        assert_eq!(amount_line(hours.first(), Unit::Imperial), "1.0 in snow");
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
        assert_eq!(compass(-45.0), "NW");
        assert_eq!(compass(360.0), "N");
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
    fn selected_forward_totals_reject_partial_measurements() {
        let mut hours = dry_hours(24);
        hours[7].precip_mm = None;

        assert_eq!(total_line(&hours, Unit::Metric), UNKNOWN);
    }

    #[test]
    fn selected_forward_totals_distinguish_zero_and_trace() {
        let zero = dry_hours(24);
        assert_eq!(total_line(&zero, Unit::Metric), "none expected");

        let mut trace = dry_hours(24);
        trace[0].precip_mm = Some(0.01);
        assert_eq!(total_line(&trace, Unit::Metric), "<0.1 mm");
        assert_eq!(total_line(&trace, Unit::Imperial), "<0.01 in");
    }

    /// The window must shorten rather than run off the end of the series.
    #[test]
    fn the_summary_window_is_clipped_at_the_end_of_the_forecast() {
        let hours = dry_hours(30);
        assert_eq!(window_from(&hours, 20).len(), 10);
        assert_eq!(window_from(&hours, 29).len(), 1);
        assert!(window_from(&hours, 30).is_empty());
    }

    #[test]
    fn the_daily_total_adds_the_window_up() {
        let mut hours = dry_hours(48);
        for hour in hours.iter_mut().take(4) {
            hour.precip_mm = Some(6.35);
        }

        assert_eq!(
            total_line(window_from(&hours, 0), Unit::Imperial),
            "1.00 in"
        );
        assert_eq!(
            total_line(window_from(&hours, 24), Unit::Imperial),
            "none expected"
        );
    }

    #[test]
    fn full_total_is_unavailable_when_every_measurement_is_missing() {
        let mut hours = dry_hours(24);
        for hour in &mut hours {
            hour.precip_mm = None;
        }

        assert_eq!(total_line(&hours, Unit::Metric), UNKNOWN);
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
        (FULL_INSPECTOR_ROWS..FULL_PAIR_ROWS)
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
        let hero = hero_temperature(hours.first(), Unit::Imperial, palette());
        let expected = big_digits("33");
        for (line, digits) in hero.iter().zip(expected) {
            assert!(line.to_string().contains(&digits), "{line:?}");
        }
        assert!(hero[DIGIT_ROWS / 2].to_string().ends_with("°F"));

        let missing = hero_temperature(None, Unit::Metric, palette());
        assert!(missing[DIGIT_ROWS / 2].to_string().ends_with("°C"));
        assert!(
            missing[DIGIT_ROWS / 2]
                .to_string()
                .contains(&big_digits("--")[DIGIT_ROWS / 2])
        );
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

        assert_eq!(panel_tops, [0, FULL_INSPECTOR_ROWS + 1]);
        assert!(
            row_text(&buffer, 100, FULL_INSPECTOR_ROWS)
                .trim()
                .is_empty(),
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

    #[test]
    fn hours_that_will_not_parse_fall_back_to_the_raw_value() {
        assert_eq!(long_hour("2026-08-10T16:00"), "Mon 10 Aug, 4:00 PM");
        assert_eq!(day_and_hour("2026-08-10T16:00"), "Mon 10 Aug, 4 PM");
        assert_eq!(long_hour("not-a-time"), "not-a-time");
    }

    #[test]
    fn the_condition_is_preserved_before_a_long_location() {
        let (city, condition) = top_titles(CITY, "Light rain", 100);
        assert_eq!(city, CITY.to_uppercase());
        assert_eq!(condition.as_deref(), Some("Light rain"));

        let (city, condition) = top_titles(CITY, "Thunderstorm, heavy hail", 48);
        assert!(
            city.ends_with('…'),
            "the location should give way: {city:?}"
        );
        assert_eq!(condition.as_deref(), Some("Thunderstorm, heavy hail"));

        let (city, condition) = top_titles(CITY, "Thunderstorm, heavy hail", 34);
        assert!(
            city.ends_with('…'),
            "the minimum should clip location: {city:?}"
        );
        assert_eq!(condition.as_deref(), Some("Thunderstorm, heavy hail"));
    }

    /// Both border rows carry two titles on the same line, so neither pair may
    /// overlap at any width the app renders at.
    #[test]
    fn border_titles_never_collide() {
        for width in 20u16..=200 {
            let available = title_room(width);

            let (city, condition) = top_titles(CITY, "Thunderstorm, heavy hail", width);
            let used = city.chars().count() + condition.map_or(0, |c| c.chars().count());
            assert!(used <= available, "top at {width}: {used} of {available}");

            let when = "Mon 10 Aug, 4:00 PM";
            let (upcoming, when) = bottom_titles("no rain in the next 8 days", when, width);
            let used = when.chars().count() + upcoming.map_or(0, |u| u.chars().count());
            assert!(
                used <= available || available < when.chars().count(),
                "bottom at {width}: {used} of {available}"
            );
        }
    }
}
