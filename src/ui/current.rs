use crate::ui::UNKNOWN;
use crate::units::Unit;
use crate::weather::code::aqi_label;
use crate::weather::model::{DailyForecast, Weather};
use chrono::NaiveDate;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub(super) fn current_area_render(
    frame: &mut Frame,
    weather: &Weather,
    area: Rect,
    unit: Unit,
    selected: usize,
) {
    // The pane doubles as the day inspector. On today it shows live current
    // conditions; on any other day it shows that day's summary, so arrowing
    // through the chart has somewhere to put its answer.
    let day = weather.daily.get(selected);
    let showing_now = selected == weather.today_index || day.is_none();

    let title = if showing_now {
        "Now".to_string()
    } else {
        day.map_or_else(|| "Now".to_string(), |d| long_date(&d.date))
    };

    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Centre the hero and details as one group rather than pinning them left.
    let [content] = Layout::horizontal([Constraint::Length(HERO_WIDTH + DETAIL_WIDTH)])
        .flex(Flex::Center)
        .areas(inner);

    let [hero_area, detail_area] = Layout::horizontal([
        Constraint::Length(HERO_WIDTH),
        Constraint::Length(DETAIL_WIDTH),
    ])
    .areas(content);

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
    // One blank row so five rows of digits sit centred in six.
    let mut hero_lines = vec![Line::from("")];
    hero_lines.extend(hero);
    frame.render_widget(
        Paragraph::new(hero_lines).alignment(Alignment::Center),
        hero_area,
    );

    // Both branches produce the same number of lines so today is no thinner
    // than any other day. Today swaps the period comparison for air quality,
    // which only exists for now; other days swap the live reading for the
    // day's feels-like range.
    let details = match day {
        Some(d) if showing_now => now_details(weather, d, unit),
        Some(d) => day_details(weather, d, unit),
        None => Vec::new(),
    };

    frame.render_widget(Paragraph::new(details), detail_area);
}

const DIGIT_ROWS: usize = 5;
/// Widths of the two columns in the "Now" pane, centred as a pair.
const HERO_WIDTH: u16 = 26;
const DETAIL_WIDTH: u16 = 30;

/// One 7x5 block glyph. At three cells wide the digits rendered far taller
/// than they were broad, since terminal cells are roughly twice as tall as
/// they are wide. Only digits and a minus sign are needed for temperatures.
fn glyph(c: char) -> [&'static str; DIGIT_ROWS] {
    match c {
        '0' => ["███████", "██   ██", "██   ██", "██   ██", "███████"],
        '1' => ["   ██  ", "   ██  ", "   ██  ", "   ██  ", "   ██  "],
        '2' => ["███████", "     ██", "███████", "██     ", "███████"],
        '3' => ["███████", "     ██", "███████", "     ██", "███████"],
        '4' => ["██   ██", "██   ██", "███████", "     ██", "     ██"],
        '5' => ["███████", "██     ", "███████", "     ██", "███████"],
        '6' => ["███████", "██     ", "███████", "██   ██", "███████"],
        '7' => ["███████", "     ██", "     ██", "     ██", "     ██"],
        '8' => ["███████", "██   ██", "███████", "██   ██", "███████"],
        '9' => ["███████", "██   ██", "███████", "     ██", "███████"],
        '-' => ["       ", "       ", "███████", "       ", "       "],
        _ => ["       "; DIGIT_ROWS],
    }
}

/// Renders `text` as block digits, one `String` per output row.
fn big_digits(text: &str) -> [String; DIGIT_ROWS] {
    let mut rows: [String; DIGIT_ROWS] = Default::default();

    for c in text.chars() {
        for (row, part) in rows.iter_mut().zip(glyph(c)) {
            row.push_str(part);
            row.push(' ');
        }
    }

    rows
}

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

fn detail_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<12}")).dark_gray(),
        Span::from(value.to_string()).white(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Every row must be the same width or Alignment::Center shears the digits,
    /// which is exactly how the hero temperature broke once before.
    #[test]
    fn block_digit_rows_are_all_the_same_width() {
        for text in ["7", "91", "107", "-12", ""] {
            let rows = big_digits(text);
            let width = rows[0].chars().count();
            for (i, row) in rows.iter().enumerate() {
                assert_eq!(row.chars().count(), width, "{text:?} row {i} differs");
            }
            assert_eq!(width, text.chars().count() * 8, "{text:?} wrong width");
        }
    }

    #[test]
    fn unknown_characters_render_as_blanks_rather_than_panicking() {
        let rows = big_digits("?");
        assert!(rows.iter().all(|r| r.trim().is_empty()));
    }

    #[test]
    fn long_date_falls_back_to_the_raw_value() {
        assert_eq!(long_date("2026-08-11"), "Tue, Aug 11");
        assert_eq!(long_date("not-a-date"), "not-a-date");
    }
}
