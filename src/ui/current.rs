use crate::ui::UNKNOWN;
use crate::units::Unit;
use crate::weather::code::aqi_label;
use crate::weather::model::{DailyForecast, Weather};
use chrono::{Local, NaiveDate};
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
    frame.render_widget(Paragraph::new(hero).alignment(Alignment::Center), hero_area);

    let details = if showing_now {
        now_details(weather, unit)
    } else {
        day.map_or_else(Vec::new, |d| day_details(weather, d, unit))
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

fn now_details(weather: &Weather, unit: Unit) -> Vec<Line<'static>> {
    let aqi = match &weather.air_quality {
        Some(aq) => format!("{} · {}", aq.us_aqi, aqi_label(aq.us_aqi)),
        None => UNKNOWN.to_string(),
    };

    let feels_like = weather.current.feels_like_c.map_or_else(
        || UNKNOWN.to_string(),
        |c| format!("{:.0}{}", unit.temp(c), unit.temp_symbol()),
    );

    let wind = weather.current.wind_kph.map_or_else(
        || UNKNOWN.to_string(),
        |kph| format!("{:.0} {}", unit.speed(kph), unit.speed_label()),
    );

    vec![
        Line::from(""),
        detail_line("feels like", &feels_like),
        detail_line("wind", &wind),
        detail_line("air quality", &aqi),
        Line::from(format!("{}", Local::now().format("%a, %b %-d  %-I:%M %p"))).dark_gray(),
    ]
}

fn day_details(weather: &Weather, day: &DailyForecast, unit: Unit) -> Vec<Line<'static>> {
    let sym = unit.temp_symbol();

    let feels = match (day.feels_max_c, day.feels_min_c) {
        (Some(hi), Some(lo)) => format!("{:.0}{sym} / {:.0}{sym}", unit.temp(hi), unit.temp(lo)),
        _ => UNKNOWN.to_string(),
    };

    let rain = match (day.precip_mm, day.precip_hours) {
        (Some(mm), Some(h)) if mm > 0.0 => format!(
            "{:.2} {} over {:.0} h",
            unit.precip(mm),
            unit.precip_label(),
            h
        ),
        (Some(_), _) => "none".to_string(),
        _ => UNKNOWN.to_string(),
    };

    let wind = match (day.wind_kph, day.gust_kph) {
        (Some(w), Some(g)) => format!(
            "{:.0}, gusts {:.0} {} {}",
            unit.speed(w),
            unit.speed(g),
            unit.speed_label(),
            day.wind_dir_deg
                .map_or(String::new(), |d| compass(d).to_string()),
        ),
        (Some(w), None) => format!("{:.0} {}", unit.speed(w), unit.speed_label()),
        _ => UNKNOWN.to_string(),
    };

    let daylight = day
        .daylight_secs
        .map_or_else(|| UNKNOWN.to_string(), duration);

    // The comparison is the part a table cannot give you: whether this day is
    // remarkable for the period, not just what its number is.
    let mean = weather.daily.iter().map(|d| d.high_c).sum::<f64>() / weather.daily.len() as f64;
    let delta = unit.temp(day.high_c) - unit.temp(mean);
    let comparison = if delta.abs() < 1.0 {
        "about average for the period".to_string()
    } else {
        format!(
            "{:.0}{sym} {} the {}-day average",
            delta.abs(),
            if delta > 0.0 { "above" } else { "below" },
            weather.daily.len(),
        )
    };

    vec![
        detail_line("feels like", &feels),
        detail_line("rain", &rain),
        detail_line("wind", &wind),
        detail_line("daylight", &daylight),
        Line::from(comparison).dark_gray(),
    ]
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
