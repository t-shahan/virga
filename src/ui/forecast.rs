use crate::units::Unit;
use crate::weather::code::emoji;
use crate::weather::model::Weather;
use chrono::{Datelike, NaiveDate};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Bar, BarChart, BarGroup, Block, Paragraph};

pub(super) fn forecast_area_render(
    frame: &mut Frame,
    weather: &Weather,
    area: Rect,
    unit: Unit,
    selected: usize,
) {
    // Today onwards. The past stays exclusive to the chart.
    let upcoming = weather.daily.get(weather.today_index..).unwrap_or(&[]);

    let block = Block::bordered().title("Forecast");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let table_area = inner;

    // Columns are dropped from the right as the table narrows, so a resize
    // costs detail rather than breaking the alignment. Decided before the area
    // is centred, or the narrowing would feed back into the column choice.
    let show_conditions = table_area.width >= TABLE_COMPACT;
    let show_all = table_area.width >= TABLE_FULL;

    // Centre the block rather than the lines: Alignment::Center would centre
    // each row on its own width, and the emoji ending every row varies in cell
    // width, so the columns would wobble line to line. Side by side this is a
    // no-op, the column already being exactly the table's width.
    let block_width = if show_all {
        TABLE_FULL
    } else if show_conditions {
        TABLE_COMPACT
    } else {
        TABLE_MINIMAL
    };
    let [table_area] = Layout::horizontal([Constraint::Length(block_width.min(table_area.width))])
        .flex(Flex::Center)
        .areas(table_area);

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
        let is_selected = weather.today_index + i == selected;

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

        // Matches the chart: yellow marks the selection, today keeps a quieter
        // tint so it stays findable once the selection has moved off it.
        let line = Line::from(row);
        if is_selected {
            line.yellow().bold()
        } else if is_today {
            line.light_blue()
        } else {
            line
        }
    }));

    frame.render_widget(Paragraph::new(lines), table_area);
}

/// The chart covers past days as well as the forecast, so it gets its own box
/// with the range in the title — that also saves the caption its own row.
pub(super) fn chart_area_render(
    frame: &mut Frame,
    weather: &Weather,
    area: Rect,
    unit: Unit,
    selected: usize,
) {
    let coolest_all = weather
        .daily
        .iter()
        .map(|d| d.high_c)
        .fold(f64::INFINITY, f64::min);
    let warmest_all = weather
        .daily
        .iter()
        .map(|d| d.high_c)
        .fold(f64::NEG_INFINITY, f64::max);

    let block = Block::bordered().title(format!(
        "Daily Highs · {:.0}–{:.0}{}",
        unit.temp(coolest_all),
        unit.temp(warmest_all),
        unit.temp_symbol(),
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Widen the bars if there's room. Anything that still doesn't fit drops the
    // oldest history first, so the forecast is never what gets clipped.
    let count = weather.daily.len().max(1);
    // Never go below MIN_BAR_STRIDE: on a cramped chart it is better to drop
    // the oldest days than to render every one of them as a single column.
    let stride =
        ((inner.width as usize + BAR_GAP as usize) / count).clamp(MIN_BAR_STRIDE as usize, 4);
    let capacity = ((inner.width as usize + BAR_GAP as usize) / stride).max(1);
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
            // Three states, not two: the selection moves, today is a fixed
            // reference, so the selection takes the loud colour.
            let day = start + i;
            let color = if day == selected {
                Color::Yellow
            } else if day == weather.today_index {
                Color::LightBlue
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

    // Centre the chart on its own measured width; left-aligned looked lopsided
    // once the bars stopped filling the pane.
    let chart_width = (visible.len() * stride).saturating_sub(BAR_GAP as usize) as u16;
    let [chart_area] = Layout::horizontal([Constraint::Length(chart_width)])
        .flex(Flex::Center)
        .areas(inner);

    frame.render_widget(
        BarChart::default()
            .data(BarGroup::default().bars(&bars))
            .max(BAR_CEILING)
            .bar_width(stride as u16 - BAR_GAP)
            .bar_gap(BAR_GAP),
        chart_area,
    );
}

// Compile-time invariants: the detail levels must stay ordered, and the split
// must never be reachable at a width the full table cannot fit. Breaking either
// fails the build rather than a test.
const _: () = assert!(TABLE_MINIMAL < TABLE_COMPACT);
const _: () = assert!(TABLE_COMPACT < TABLE_FULL);
const _: () = assert!(SIDE_BY_SIDE_MIN > TABLE_FULL + GUTTER);

/// Rendered width of the table at each level of detail, emoji included.
pub(super) const TABLE_MINIMAL: u16 = 26;
const TABLE_COMPACT: u16 = 42;
pub(super) const TABLE_FULL: u16 = 68;
/// Bars thinner than this read as a comb rather than a chart.
const MIN_BAR_STRIDE: u16 = 3;
/// Every day at a stride that still looks like a bar chart.
const CHART_COMFORTABLE: u16 = 22 * MIN_BAR_STRIDE - BAR_GAP;
/// Below this the table and chart stack. Splitting only pays when the table
/// keeps every column *and* the chart keeps readable bars — at the old
/// threshold the chart fell to one-cell bars the moment the split kicked in.
pub(super) const SIDE_BY_SIDE_MIN: u16 = TABLE_FULL + GUTTER + CHART_COMFORTABLE;

const DASH: &str = "–";

/// Past this the bars grow spindly rather than informative.
pub(super) const CHART_MAX: u16 = 16;
/// Below this the bars stop being readable at all.
pub(super) const CHART_MIN: u16 = 8;
/// Columns between the table and the chart when side by side.
pub(super) const GUTTER: u16 = 3;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Unit;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn weekday_falls_back_to_the_raw_value() {
        assert_eq!(weekday("2026-08-11"), "Tue");
        assert_eq!(weekday("nonsense"), "nonsense");
    }

    #[test]
    fn clock_slices_the_time_out_of_a_timestamp() {
        assert_eq!(clock("2026-08-09T06:17"), "06:17");
        assert_eq!(clock("short"), "short");
    }

    /// Bars narrower than two cells read as a comb. This has regressed twice,
    /// once via the split threshold and once via the stride clamp.
    #[test]
    fn bars_never_fall_below_two_cells() {
        for width in 30u16..=250 {
            let stride =
                ((width as usize + BAR_GAP as usize) / 22).clamp(MIN_BAR_STRIDE as usize, 4);
            assert!(
                stride as u16 - BAR_GAP >= 2,
                "width {width} gives {}-cell bars",
                stride as u16 - BAR_GAP
            );
        }
    }

    /// Whatever is dropped for want of room, the bars that remain must fit the
    /// area they are given — anything wider is silently clipped.
    #[test]
    fn the_visible_bars_always_fit_their_area() {
        for width in 30usize..=250 {
            let stride = ((width + BAR_GAP as usize) / 22).clamp(MIN_BAR_STRIDE as usize, 4);
            let capacity = ((width + BAR_GAP as usize) / stride).max(1);
            let shown = capacity.min(22);
            let used = shown * stride - BAR_GAP as usize;
            assert!(used <= width, "width {width}: bars need {used}");
        }
    }

    /// Every size that has broken at some point, plus the boundaries either
    /// side of the split. Rendering must not panic or overrun the buffer.
    #[test]
    fn renders_without_panicking_at_awkward_sizes() {
        let w = Weather::fixture(22, 14);
        for (width, height) in [
            (40, 12),
            (70, 30),
            (100, 30),
            (113, 40),
            (114, 40),
            (135, 40),
            (136, 24),
            (138, 20),
            (200, 50),
        ] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| {
                let [top, bottom] = Layout::vertical([
                    Constraint::Length(f.area().height / 2),
                    Constraint::Fill(1),
                ])
                .areas(f.area());
                forecast_area_render(f, &w, top, Unit::Imperial, 14);
                chart_area_render(f, &w, bottom, Unit::Imperial, 14);
            })
            .unwrap();
        }
    }
}
