use crate::units::Unit;
use crate::weather::code::emoji;
use crate::weather::model::Weather;
use chrono::{Datelike, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Paragraph};

pub(super) fn forecast_area_render(frame: &mut Frame, weather: &Weather, area: Rect, unit: Unit) {
    let block = Block::bordered().title("Forecast");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Today onwards. The past stays exclusive to the chart.
    let upcoming = weather.daily.get(weather.today_index..).unwrap_or(&[]);

    let table_rows = upcoming.len() as u16 + 1;

    // Side by side once both halves can be useful, otherwise stack. The table
    // takes its full column set only when the chart can still show every day;
    // below that the chart is the better use of the width, so the table drops
    // to its compact form.
    let (table_area, caption_area, chart_area) = if inner.width >= SIDE_BY_SIDE_MIN {
        let table_width = if inner.width >= FULL_TABLE_AND_CHART {
            TABLE_FULL
        } else {
            TABLE_COMPACT
        };

        let [left, right] =
            Layout::horizontal([Constraint::Length(table_width), Constraint::Fill(1)]).areas(inner);
        let [caption, chart] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(CHART_HEIGHT)])
                .areas(right);

        (left, caption, chart)
    } else {
        // The table is fixed-size and cannot use spare rows; the chart gains
        // resolution from every one it gets, so the chart is what flexes.
        let [table, caption, chart] = Layout::vertical([
            Constraint::Length(table_rows),
            Constraint::Length(1),
            Constraint::Length(CHART_HEIGHT),
        ])
        .areas(inner);

        (table, caption, chart)
    };

    // Columns are dropped from the right as the table narrows, so a resize
    // costs detail rather than breaking the alignment.
    let show_conditions = table_area.width >= TABLE_COMPACT;
    let show_all = table_area.width >= TABLE_FULL;

    // Emoji cell widths vary between glyphs, so the icon sits at the end of the
    // row where it cannot push the numeric columns out of alignment.
    let mut header = format!("  {:<5}{:>7}{:>8}", "day", "high", "low");
    if show_conditions {
        header += &format!("{:>7}{:>8}", "rain", "wind");
    }
    if show_all {
        header += &format!("{:>7}{:>10}{:>9}", "uv", "sunrise", "sunset");
    }

    let mut lines = vec![Line::from(header).dark_gray()];

    lines.extend(upcoming.iter().enumerate().map(|(i, d)| {
        let is_today = i == 0;

        let day = if is_today {
            "Today".to_string()
        } else {
            weekday(&d.date)
        };

        // The high/low widths in the heading are these value widths plus the
        // two-cell unit symbol, so the headings sit over their own columns.
        let mut row = format!(
            "  {:<5}{:>5.0}{}{:>6.0}{}",
            day,
            unit.temp(d.high_c),
            unit.temp_symbol(),
            unit.temp(d.low_c),
            unit.temp_symbol(),
        );

        if show_conditions {
            let rain = d
                .rain_chance
                .map_or_else(|| DASH.to_string(), |p| format!("{p}%"));
            let wind = d.wind_kph.map_or_else(
                || DASH.to_string(),
                |kph| format!("{:.0} {}", unit.speed(kph), unit.speed_label()),
            );
            row += &format!("{rain:>7}{wind:>8}");
        }

        if show_all {
            let uv = d
                .uv_index
                .map_or_else(|| DASH.to_string(), |uv| format!("{uv:.0}"));
            let sunrise = d.sunrise.as_deref().map_or(DASH, clock);
            let sunset = d.sunset.as_deref().map_or(DASH, clock);
            row += &format!("{uv:>7}{sunrise:>10}{sunset:>9}");
        }

        row += &format!("   {}", emoji(d.code));

        // Same yellow as today's bar in the chart, so the two read as the
        // same day.
        let line = Line::from(row);
        if is_today { line.yellow() } else { line }
    }));

    frame.render_widget(Paragraph::new(lines), table_area);

    // Widen the bars if there's room. Anything that still doesn't fit drops the
    // oldest history first, so the forecast is never what gets clipped.
    let count = weather.daily.len().max(1);
    let stride = ((chart_area.width as usize + BAR_GAP as usize) / count).clamp(2, 4);
    let capacity = ((chart_area.width as usize + BAR_GAP as usize) / stride).max(1);
    let start = weather.daily.len().saturating_sub(capacity);
    let visible = &weather.daily[start..];

    let coolest = visible
        .iter()
        .map(|d| d.high_c)
        .fold(f64::INFINITY, f64::min);
    let warmest = visible
        .iter()
        .map(|d| d.high_c)
        .fold(f64::NEG_INFINITY, f64::max);

    // Map the observed range onto BAR_FLOOR..=BAR_CEILING rather than 0..=max.
    // Scaling from zero flattens a week of similar highs into identical bars,
    // and a bar worth a few percent of the tallest rounds down to nothing.
    let span = (warmest - coolest).max(0.1);
    let scale = (BAR_CEILING - BAR_FLOOR) as f64;

    let bars: Vec<Bar> = visible
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let color = if start + i == weather.today_index {
                Color::Yellow
            } else {
                Color::Blue
            };
            let value = BAR_FLOOR + (((d.high_c - coolest) / span) * scale).round() as u64;
            Bar::default()
                .value(value)
                .text_value(String::new())
                .style(Style::new().fg(color))
        })
        .collect();

    let caption = format!(
        "daily high · {:.0}–{:.0}{}",
        unit.temp(coolest),
        unit.temp(warmest),
        unit.temp_symbol(),
    );
    frame.render_widget(
        Paragraph::new(Line::from(caption).dark_gray()).alignment(Alignment::Center),
        caption_area,
    );

    // Centre the chart on its own measured width; left-aligned looked lopsided
    // once the bars stopped filling the pane.
    let chart_width = (visible.len() * stride).saturating_sub(BAR_GAP as usize) as u16;
    let [chart_area] = Layout::horizontal([Constraint::Length(chart_width)])
        .flex(Flex::Center)
        .areas(chart_area);

    frame.render_widget(
        BarChart::default()
            .data(BarGroup::default().bars(&bars))
            .max(BAR_CEILING)
            .bar_width(stride as u16 - BAR_GAP)
            .bar_gap(BAR_GAP),
        chart_area,
    );
}

/// Rows the pane needs including its border, so the caller can size it to its
/// content. Left to Fill it swallowed every spare row on a tall window, which
/// stretched the bars into towers and left a void beneath the table.
pub(super) fn height(weather: &Weather, width: u16) -> u16 {
    let table_rows = weather.daily.len().saturating_sub(weather.today_index) as u16 + 1;
    let inner = if width.saturating_sub(2) >= SIDE_BY_SIDE_MIN {
        table_rows.max(1 + CHART_HEIGHT)
    } else {
        table_rows + 1 + CHART_HEIGHT
    };

    inner + 2
}

/// Rendered width of the table at each level of detail, emoji included.
const TABLE_COMPACT: u16 = 42;
const TABLE_FULL: u16 = 68;
/// Below this the table and chart stack instead of sitting side by side.
const SIDE_BY_SIDE_MIN: u16 = 70;
/// Enough for the full table beside a chart that can still show every day.
const FULL_TABLE_AND_CHART: u16 = 113;

const DASH: &str = "–";

const CHART_HEIGHT: u16 = 10;

const BAR_GAP: u16 = 1;
/// Shortest bar, as a proportion of `BAR_CEILING`. Keeps the coolest day visible.
const BAR_FLOOR: u64 = 15;
const BAR_CEILING: u64 = 100;

/// "2026-08-09T06:17" -> "06:17". Falls back to the raw value if the shape
/// is not what we expect, rather than guessing.
fn clock(stamp: &str) -> &str {
    stamp.get(11..16).unwrap_or(stamp)
}

fn weekday(date: &str) -> String {
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(parsed) => parsed.weekday().to_string(),
        Err(_) => date.to_string(),
    }
}
