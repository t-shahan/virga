use crate::ui::UNKNOWN;
use crate::units::Unit;
use crate::weather::code::aqi_label;
use crate::weather::model::Weather;
use chrono::Local;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub(super) fn current_area_render(frame: &mut Frame, weather: &Weather, area: Rect, unit: Unit) {
    let block = Block::bordered().title("Now");
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
    let temp = weather
        .current
        .temp_c
        .map_or_else(|| "--".to_string(), |c| format!("{:.0}", unit.temp(c)));

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
                    Span::from(row.clone()).bold().cyan(),
                    Span::from(symbol.to_string()).cyan(),
                ])
            } else {
                Line::from(format!("{row}{symbol_pad}")).bold().cyan()
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(hero).alignment(Alignment::Center), hero_area);

    let aqi = match &weather.air_quality {
        Some(aq) => format!("{} · {}", aq.us_aqi, aqi_label(aq.us_aqi)),
        None => "unavailable".to_string(),
    };

    let feels_like = weather.current.feels_like_c.map_or_else(
        || UNKNOWN.to_string(),
        |c| format!("{:.0}{}", unit.temp(c), unit.temp_symbol()),
    );

    let wind = weather.current.wind_kph.map_or_else(
        || UNKNOWN.to_string(),
        |kph| format!("{:.0} {}", unit.speed(kph), unit.speed_label()),
    );

    let details = vec![
        Line::from(""),
        detail_line("feels like", &feels_like),
        detail_line("wind", &wind),
        detail_line("air quality", &aqi),
        Line::from(format!("{}", Local::now().format("%a, %b %-d  %-I:%M %p"))).dark_gray(),
    ];
    frame.render_widget(Paragraph::new(details), detail_area);
}

const DIGIT_ROWS: usize = 5;
/// Widths of the two columns in the "Now" pane, centred as a pair.
const HERO_WIDTH: u16 = 16;
const DETAIL_WIDTH: u16 = 30;

/// One 3x5 block glyph. Only digits and a minus sign are needed for temperatures.
fn glyph(c: char) -> [&'static str; DIGIT_ROWS] {
    match c {
        '0' => ["███", "█ █", "█ █", "█ █", "███"],
        '1' => ["  █", "  █", "  █", "  █", "  █"],
        '2' => ["███", "  █", "███", "█  ", "███"],
        '3' => ["███", "  █", "███", "  █", "███"],
        '4' => ["█ █", "█ █", "███", "  █", "  █"],
        '5' => ["███", "█  ", "███", "  █", "███"],
        '6' => ["███", "█  ", "███", "█ █", "███"],
        '7' => ["███", "  █", "  █", "  █", "  █"],
        '8' => ["███", "█ █", "███", "█ █", "███"],
        '9' => ["███", "█ █", "███", "  █", "███"],
        '-' => ["   ", "   ", "███", "   ", "   "],
        _ => ["   ", "   ", "   ", "   ", "   "],
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

fn detail_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<12}")).dark_gray(),
        Span::from(value.to_string()).white(),
    ])
}
