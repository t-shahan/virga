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

    let [list_area, caption_area, chart_area] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(7),
    ])
    .areas(inner);

    // The list is the forecast only; today and the past belong to the chart.
    let upcoming = weather.daily.get(weather.today_index + 1..).unwrap_or(&[]);

    // Emoji cell widths vary between glyphs, so the icon sits at the end of the
    // row where it cannot push the numeric columns out of alignment.
    let mut lines = vec![
        // The high/low widths here are the value width plus the two-cell unit
        // symbol the rows append, so the headings sit over their own columns.
        Line::from(format!(
            "  {:<5}{:>7}{:>8}{:>7}{:>8}{:>7}",
            "day", "high", "low", "rain", "wind", "uv"
        ))
        .dark_gray(),
    ];

    lines.extend(upcoming.iter().map(|d| {
        let rain = d
            .rain_chance
            .map_or_else(|| "–".to_string(), |p| format!("{p}%"));
        let wind = d.wind_kph.map_or_else(
            || "–".to_string(),
            |kph| format!("{:.0} {}", unit.speed(kph), unit.speed_label()),
        );
        let uv = d
            .uv_index
            .map_or_else(|| "–".to_string(), |uv| format!("{uv:.0}"));

        Line::from(format!(
            "  {:<5}{:>5.0}{}{:>6.0}{}{:>7}{:>8}{:>7}   {}",
            weekday(&d.date),
            unit.temp(d.high_c),
            unit.temp_symbol(),
            unit.temp(d.low_c),
            unit.temp_symbol(),
            rain,
            wind,
            uv,
            emoji(d.code),
        ))
    }));

    frame.render_widget(Paragraph::new(lines), list_area);

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
                Color::Cyan
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

const BAR_GAP: u16 = 1;
/// Shortest bar, as a proportion of `BAR_CEILING`. Keeps the coolest day visible.
const BAR_FLOOR: u64 = 15;
const BAR_CEILING: u64 = 100;

fn weekday(date: &str) -> String {
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(parsed) => parsed.weekday().to_string(),
        Err(_) => date.to_string(),
    }
}
