use crate::app::App;
use crate::ui::UNKNOWN;
use crate::ui::digits::{CELL_WIDTH, DIGIT_ROWS, big_digits};
use crate::units::Unit;
use crate::weather::code::{aqi_label, description};
use crate::weather::model::{DailyForecast, Weather};
use chrono::NaiveDate;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub(super) fn current_area_render(frame: &mut Frame, app: &App, weather: &Weather, area: Rect) {
    let unit = app.unit;
    let selected = app.selected_day;
    // The pane doubles as the day inspector. On today it shows live current
    // conditions; on any other day it shows that day's summary, so arrowing
    // through the chart has somewhere to put its answer.
    let day = weather.daily.get(selected);
    let showing_now = selected == weather.today_index || day.is_none();

    let when = if showing_now {
        "Now".to_string()
    } else {
        day.map_or_else(|| "Now".to_string(), |d| long_date(&d.date))
    };

    let name = app.location.as_deref().unwrap_or(&weather.location);

    let condition = if showing_now {
        weather.current.code.map_or(UNKNOWN, description)
    } else {
        day.map_or(UNKNOWN, |d| description(d.code))
    };

    // Both ends of the border carry text, so the identity costs no interior
    // rows at all — the chrome is being drawn either way. Nothing stops the two
    // titles overwriting each other, so the budget is worked out explicitly.
    let (left, right) = border_titles(name, condition, &when, area.width);

    let block = Block::bordered()
        .title_top(Line::from(left).bold().blue().left_aligned())
        .title_top(Line::from(right).dark_gray().right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The block digits are decoration; the readings are the content. Rather
    // than squeeze both and clip the digits mid-glyph, drop the hero entirely
    // once there is not room for the pair.
    let full = HERO_WIDTH + COLUMN_GUTTER + DETAIL_WIDTH;
    let show_hero = inner.width >= full;

    let wanted = if show_hero {
        full
    } else {
        DETAIL_WIDTH.min(inner.width)
    };

    // Centre the group rather than pinning it left.
    let [content] = Layout::horizontal([Constraint::Length(wanted)])
        .flex(Flex::Center)
        .areas(inner);

    // Without a gutter a three-digit temperature fills HERO_WIDTH exactly and
    // its unit symbol lands flush against the first label.
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

    // The block font already has a '-' glyph, so a missing reading renders as
    // "--" at the same scale rather than collapsing the layout.
    let temp = if showing_now {
        weather
            .current
            .temp_c
            .map_or_else(|| "--".to_string(), |c| format!("{:.0}", unit.temp(c)))
    } else {
        day.map_or_else(
            || "--".to_string(),
            |d| format!("{:.0}", unit.temp(d.high_c)),
        )
    };

    // Alignment::Center centres each line on its own width, so the row carrying
    // the unit symbol would sit offset from the rest. Pad the others to match.
    let symbol = unit.temp_symbol();
    let symbol_pad = " ".repeat(symbol.chars().count());

    let hero: Vec<Line> = big_digits(&temp)
        .iter()
        .enumerate()
        .map(|(i, row)| {
            // Hang the unit symbol off the middle row so it sits centred against the digits.
            if i == DIGIT_ROWS / 2 {
                Line::from(vec![
                    Span::from(row.clone()).bold().blue(),
                    Span::from(symbol.to_string()).blue(),
                ])
            } else {
                Line::from(format!("{row}{symbol_pad}")).bold().blue()
            }
        })
        .collect();
    let mut hero_lines = vec![Line::from("")];
    hero_lines.extend(hero);

    if let Some(hero_area) = hero_area {
        frame.render_widget(
            Paragraph::new(hero_lines).alignment(Alignment::Center),
            hero_area,
        );
    }

    // Both branches produce the same number of lines so today is no thinner
    // than any other day. Today swaps the period comparison for air quality,
    // which only exists for now; other days swap the live reading for the
    // day's feels-like range.
    // A leading blank keeps the labels level with the digits.
    let mut details = vec![Line::from("")];
    details.extend(match day {
        Some(d) if showing_now => now_details(weather, d, unit),
        Some(d) => day_details(weather, d, unit),
        None => Vec::new(),
    });

    frame.render_widget(Paragraph::new(details), detail_area);
}

/// Widths of the two columns in the "Now" pane, centred as a pair. The hero
/// holds three digits and the unit symbol, sized from the font rather than
/// restated here.
const HERO_WIDTH: u16 = 3 * CELL_WIDTH + 2;
const DETAIL_WIDTH: u16 = 30;
/// Columns between the hero and the details.
const COLUMN_GUTTER: u16 = 3;
/// Columns kept clear between the two border titles.
const TITLE_GUTTER: usize = 3;

fn now_details(weather: &Weather, today: &DailyForecast, unit: Unit) -> Vec<Line<'static>> {
    let sym = unit.temp_symbol();

    let aqi = match &weather.air_quality {
        Some(aq) => format!("{} · {}", aq.us_aqi, aqi_label(aq.us_aqi)),
        None => UNKNOWN.to_string(),
    };

    let feels_like = weather.current.feels_like_c.map_or_else(
        || UNKNOWN.to_string(),
        |c| format!("{:.0}{sym}", unit.temp(c)),
    );

    let wind = wind_line(
        today.wind_kph.or(weather.current.wind_kph),
        today.gust_kph,
        today,
        unit,
    );

    vec![
        detail_line("feels like", &feels_like),
        detail_line("high / low", &high_low(today, unit)),
        detail_line("rain", &rain_line(today, unit)),
        detail_line("wind", &wind),
        detail_line("daylight", &daylight_line(today)),
        detail_line("air quality", &aqi),
    ]
}

fn day_details(weather: &Weather, day: &DailyForecast, unit: Unit) -> Vec<Line<'static>> {
    let sym = unit.temp_symbol();

    let feels = match (day.feels_max_c, day.feels_min_c) {
        (Some(hi), Some(lo)) => format!("{:.0}{sym} / {:.0}{sym}", unit.temp(hi), unit.temp(lo)),
        _ => UNKNOWN.to_string(),
    };

    vec![
        detail_line("feels like", &feels),
        detail_line("high / low", &high_low(day, unit)),
        detail_line("rain", &rain_line(day, unit)),
        detail_line("wind", &wind_line(day.wind_kph, day.gust_kph, day, unit)),
        detail_line("daylight", &daylight_line(day)),
        Line::from(comparison(weather, day, unit)).dark_gray(),
    ]
}

fn high_low(day: &DailyForecast, unit: Unit) -> String {
    let sym = unit.temp_symbol();
    format!(
        "{:.0}{sym} / {:.0}{sym}",
        unit.temp(day.high_c),
        unit.temp(day.low_c)
    )
}

fn rain_line(day: &DailyForecast, unit: Unit) -> String {
    match (day.precip_mm, day.precip_hours) {
        (Some(mm), Some(h)) if mm > 0.0 => format!(
            "{:.2} {} over {:.0} h",
            unit.precip(mm),
            unit.precip_label(),
            h
        ),
        (Some(_), _) => "none".to_string(),
        _ => UNKNOWN.to_string(),
    }
}

fn wind_line(speed: Option<f64>, gust: Option<f64>, day: &DailyForecast, unit: Unit) -> String {
    let direction = day
        .wind_dir_deg
        .map_or(String::new(), |d| format!(" {}", compass(d)));

    match (speed, gust) {
        (Some(w), Some(g)) => format!(
            "{:.0}, gusts {:.0} {}{direction}",
            unit.speed(w),
            unit.speed(g),
            unit.speed_label(),
        ),
        (Some(w), None) => format!("{:.0} {}{direction}", unit.speed(w), unit.speed_label()),
        _ => UNKNOWN.to_string(),
    }
}

fn daylight_line(day: &DailyForecast) -> String {
    day.daylight_secs
        .map_or_else(|| UNKNOWN.to_string(), duration)
}

/// The part a table cannot give you: whether the day is remarkable for the
/// period, not merely what its number is.
fn comparison(weather: &Weather, day: &DailyForecast, unit: Unit) -> String {
    if weather.daily.is_empty() {
        return String::new();
    }

    let mean = weather.daily.iter().map(|d| d.high_c).sum::<f64>() / weather.daily.len() as f64;
    let delta = unit.temp(day.high_c) - unit.temp(mean);

    if delta.abs() < 1.0 {
        "about average for the period".to_string()
    } else {
        format!(
            "{:.0}{} {} the {}-day average",
            delta.abs(),
            unit.temp_symbol(),
            if delta > 0.0 { "above" } else { "below" },
            weather.daily.len(),
        )
    }
}

/// Sixteen points is more precision than a daily dominant direction deserves.
fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    POINTS[(((degrees % 360.0 + 360.0) % 360.0 / 45.0).round() as usize) % 8]
}

fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64 / 60;
    format!("{}h {}m", total / 60, total % 60)
}

fn long_date(date: &str) -> String {
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(parsed) => parsed.format("%a, %b %-d").to_string(),
        Err(_) => date.to_string(),
    }
}

/// Fits the city, condition and day into the border, dropping detail rather
/// than letting the two titles overwrite each other. The day matters most —
/// it is what changes as you arrow around — then the city, then the condition.
fn border_titles(name: &str, condition: &str, when: &str, width: u16) -> (String, String) {
    // Two corners, plus a column of breathing room inside each.
    let available = width.saturating_sub(4) as usize;
    let name = name.to_uppercase();
    let len = |s: &str| s.chars().count();

    let full = format!("{condition} · {when}");
    if len(&name) + TITLE_GUTTER + len(&full) <= available {
        return (name, full);
    }

    if len(&name) + TITLE_GUTTER + len(when) <= available {
        return (name, when.to_string());
    }

    let room = available.saturating_sub(TITLE_GUTTER + len(when));
    (truncate(&name, room), when.to_string())
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

fn detail_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<12}")).dark_gray(),
        Span::from(value.to_string()).white(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Fetch;

    use crate::units::Unit;
    use crate::weather::model::Weather;

    /// Today used to carry fewer lines than any other day, which read as a gap
    /// rather than a difference.
    #[test]
    fn today_and_other_days_carry_the_same_number_of_lines() {
        let w = Weather::fixture(22, 14);
        let today = &w.daily[14];
        let other = &w.daily[3];

        assert_eq!(
            now_details(&w, today, Unit::Imperial).len(),
            day_details(&w, other, Unit::Imperial).len()
        );
    }

    #[test]
    fn comparison_names_the_direction_from_the_average() {
        let w = Weather::fixture(21, 10);
        // Fixture highs climb 20..40, so the mean is the middle day.
        assert!(comparison(&w, &w.daily[20], Unit::Metric).contains("above"));
        assert!(comparison(&w, &w.daily[0], Unit::Metric).contains("below"));
        assert!(comparison(&w, &w.daily[10], Unit::Metric).contains("average"));
    }

    #[test]
    fn comparison_is_empty_rather_than_dividing_by_zero() {
        let w = Weather::fixture(0, 0);
        let day = Weather::fixture(1, 0).daily.remove(0);
        assert_eq!(comparison(&w, &day, Unit::Metric), "");
    }

    #[test]
    fn missing_readings_read_as_unavailable_rather_than_blank() {
        let mut w = Weather::fixture(3, 1);
        let day = &mut w.daily[1];
        day.precip_mm = None;
        day.gust_kph = None;
        day.wind_kph = None;
        day.daylight_secs = None;

        assert_eq!(rain_line(&w.daily[1], Unit::Metric), UNKNOWN);
        assert_eq!(daylight_line(&w.daily[1]), UNKNOWN);
        assert_eq!(wind_line(None, None, &w.daily[1], Unit::Metric), UNKNOWN);
    }

    const CITY: &str = "Frederick, Maryland, United States";

    #[test]
    fn border_shows_everything_when_there_is_room() {
        let (left, right) = border_titles(CITY, "Drizzle", "Sun, Aug 16", 120);
        assert_eq!(left, CITY.to_uppercase());
        assert_eq!(right, "Drizzle · Sun, Aug 16");
    }

    /// The condition is the first thing to go: the day is what changes as you
    /// arrow around, and the city is the identity.
    #[test]
    fn border_drops_the_condition_before_the_day() {
        let (left, right) = border_titles(CITY, "Thunderstorm, heavy hail", "Sun, Aug 16", 60);
        assert_eq!(left, CITY.to_uppercase());
        assert_eq!(right, "Sun, Aug 16");
    }

    #[test]
    fn border_clips_the_city_last_and_keeps_the_day_whole() {
        let (left, right) = border_titles(CITY, "Drizzle", "Sun, Aug 16", 30);
        assert_eq!(right, "Sun, Aug 16", "the day is never sacrificed");
        assert!(left.ends_with('…'), "the city took the cut: {left:?}");
    }

    /// Whatever is shown must leave a gap between the two titles at every width
    /// the app will render at — they are drawn onto the same border row.
    #[test]
    fn border_titles_never_collide() {
        for width in 20u16..=200 {
            let (left, right) =
                border_titles(CITY, "Thunderstorm, heavy hail", "Sun, Aug 16", width);
            let used = left.chars().count() + right.chars().count();
            let available = width.saturating_sub(4) as usize;
            assert!(
                used + TITLE_GUTTER <= available || left.is_empty(),
                "width {width}: {left:?} + {right:?} needs {} of {available}",
                used + TITLE_GUTTER
            );
        }
    }

    /// The budget is arithmetic; this checks what ratatui actually draws.
    #[test]
    fn rendered_border_keeps_the_day_and_never_overlaps() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.selected_day = 20;
        app.location = Some(CITY.to_string());

        for width in [44u16, 60, 80, 120, 200] {
            let mut t = Terminal::new(TestBackend::new(width, 9)).unwrap();
            t.draw(|f| current_area_render(f, &app, &Weather::fixture(22, 14), f.area()))
                .unwrap();

            let buf = t.backend().buffer();
            let top: String = (0..width).map(|x| buf[(x, 0)].symbol()).collect();

            assert!(
                top.contains("Aug"),
                "width {width}: day missing from {top:?}"
            );
            assert!(
                !top.contains("FREDERICKAug") && !top.contains("STATESAug"),
                "width {width}: titles ran together in {top:?}"
            );
        }
    }

    #[test]
    fn truncate_marks_the_cut_and_leaves_short_text_alone() {
        assert_eq!(truncate("Clear sky", 26), "Clear sky");
        assert_eq!(
            truncate("Thunderstorm, heavy hail", 26),
            "Thunderstorm, heavy hail"
        );
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("", 4), "");
    }

    /// Narrow panes drop the block digits rather than clipping them mid-glyph.
    /// Whatever happens to the decoration, every reading must survive intact.
    #[test]
    fn narrow_panes_keep_the_readings_and_drop_the_digits() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.location = Some(CITY.to_string());

        for width in [34u16, 44, 58, 59, 80] {
            let mut t = Terminal::new(TestBackend::new(width, 9)).unwrap();
            t.draw(|f| current_area_render(f, &app, &Weather::fixture(22, 14), f.area()))
                .unwrap();

            let buf = t.backend().buffer();
            let text: String = (0..9u16)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| buf[(x, y)].symbol())
                .collect();

            for label in ["feels like", "high / low", "daylight"] {
                assert!(text.contains(label), "width {width} lost {label:?}");
            }
        }
    }

    #[test]
    fn compass_maps_degrees_to_points() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(45.0), "NE");
        assert_eq!(compass(90.0), "E");
        assert_eq!(compass(180.0), "S");
        assert_eq!(compass(315.0), "NW");
    }

    /// 360 must not index past the end of the table, and the API has been known
    /// to report a hair over or under.
    #[test]
    fn compass_wraps_at_the_full_circle() {
        assert_eq!(compass(360.0), "N");
        assert_eq!(compass(359.0), "N");
        assert_eq!(compass(720.0), "N");
        assert_eq!(compass(-45.0), "NW");
    }

    #[test]
    fn duration_splits_seconds_into_hours_and_minutes() {
        assert_eq!(duration(49_320.0), "13h 42m");
        assert_eq!(duration(3_600.0), "1h 0m");
        assert_eq!(duration(0.0), "0h 0m");
        assert_eq!(duration(-5.0), "0h 0m");
    }

    #[test]
    fn long_date_falls_back_to_the_raw_value() {
        assert_eq!(long_date("2026-08-11"), "Tue, Aug 11");
        assert_eq!(long_date("not-a-date"), "not-a-date");
    }
}
