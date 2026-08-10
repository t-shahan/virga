use crate::units::Unit;
use crate::weather::model::Weather;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Bar, BarChart, BarGroup, Block};

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

/// Bars thinner than this read as a comb rather than a chart.
const MIN_BAR_STRIDE: u16 = 3;
const BAR_GAP: u16 = 1;
/// Shortest bar, as a proportion of `BAR_CEILING`. Keeps the coolest day visible.
const BAR_FLOOR: u64 = 15;
const BAR_CEILING: u64 = 100;

/// Every day at a stride that still looks like a bar chart. The caller uses
/// this to decide whether a side-by-side split can afford a chart at all.
pub(super) const COMFORTABLE_WIDTH: u16 = 22 * MIN_BAR_STRIDE - BAR_GAP;
/// Below this the bars stop being readable at all.
pub(super) const MIN_HEIGHT: u16 = 8;
/// Past this they grow spindly rather than informative.
pub(super) const MAX_HEIGHT: u16 = 16;

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

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

    #[test]
    fn renders_without_panicking_at_awkward_sizes() {
        let w = Weather::fixture(22, 14);
        for (width, height) in [(40, 12), (70, 30), (136, 24), (138, 20), (200, 50)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| chart_area_render(f, &w, f.area(), Unit::Imperial, 14))
                .unwrap();
        }
    }
}
