use crate::theme::Palette;
use crate::ui::bars::{Columns, GAP};
use crate::units::Unit;
use crate::weather::model::Weather;
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Bar, BarChart, BarGroup, Block};

/// The chart covers past days as well as the forecast, so it gets its own box
/// with the range in the title — that also saves the caption its own row.
pub(super) fn chart_area_render(
    frame: &mut Frame,
    weather: &Weather,
    palette: Palette,
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

    let block = Block::bordered()
        .border_style(Style::new().fg(palette.border))
        // Styled rather than left to the block: a title takes the block's own
        // style, not the border's, so unstyled it ignores the theme entirely.
        .title(
            Line::from(format!(
                "Daily Highs · {:.0}–{:.0}{}",
                unit.temp(coolest_all),
                unit.temp(warmest_all),
                unit.temp_symbol(),
            ))
            .fg(palette.muted),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Anything that doesn't fit drops the oldest history first, so the
    // forecast is never what gets clipped.
    let columns = Columns::fit(
        inner.width,
        weather.daily.len(),
        MIN_BAR_STRIDE,
        MAX_BAR_STRIDE,
    );
    let start = weather.daily.len().saturating_sub(columns.capacity);
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
                palette.selection
            } else if day == weather.today_index {
                palette.now
            } else {
                palette.series
            };
            let value = BAR_FLOOR + (((d.high_c - coolest) / span) * scale).round() as u64;
            Bar::default()
                .value(value)
                .text_value(String::new())
                .style(Style::new().fg(color))
        })
        .collect();

    // The last row carries the selection marker. Colour alone put the
    // selection out of reach of a monochrome terminal, a colour-blind reader,
    // or a screenshot — and the bars cannot carry a shape the way the
    // precipitation chart's centre rule can, so the marker needs its own row.
    // One row out of six at the smallest chart the layout will draw.
    let (bars_row_area, marker_row) = if inner.height > 2 {
        let [bars, marker] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
        (bars, Some(marker))
    } else {
        (inner, None)
    };

    // Centre the chart on its own measured width; left-aligned looked lopsided
    // once the bars stopped filling the pane.
    let [chart_area] = Layout::horizontal([Constraint::Length(columns.width_of(visible.len()))])
        .flex(Flex::Center)
        .areas(bars_row_area);

    if let Some(marker_row) = marker_row
        && let Some(offset) =
            caret_offset(&columns, selected, start, visible.len(), chart_area.width)
    {
        let cell = Rect {
            x: chart_area.x + offset,
            y: marker_row.y,
            width: 1,
            height: 1,
        };
        frame.render_widget(
            Line::from("^").style(Style::new().fg(palette.selection)),
            cell,
        );
    }

    frame.render_widget(
        BarChart::default()
            .data(BarGroup::default().bars(&bars))
            .max(BAR_CEILING)
            .bar_width(columns.stride - GAP)
            .bar_gap(GAP),
        chart_area,
    );
}

/// How far into the chart the selection marker goes, if it goes anywhere.
///
/// The selection can sit outside the drawn window — the chart drops the oldest
/// history first when the pane is narrow — in which case there is nothing to
/// point at and the row stays blank rather than pointing at the wrong day.
fn caret_offset(
    columns: &Columns,
    selected: usize,
    start: usize,
    shown: usize,
    width: u16,
) -> Option<u16> {
    let column = selected.checked_sub(start).filter(|c| *c < shown)?;

    // Centre it under the bar rather than its stride, so a two-cell bar with a
    // one-cell gap gets a caret over the bar and not over the gap beside it.
    let bar_width = columns.stride - GAP;
    let offset = column as u16 * columns.stride + bar_width / 2;

    (offset < width).then_some(offset)
}

/// Bars thinner than this read as a comb rather than a chart.
const MIN_BAR_STRIDE: u16 = 3;
/// Past this they grow fat rather than more informative.
const MAX_BAR_STRIDE: u16 = 4;
/// Shortest bar, as a proportion of `BAR_CEILING`. Keeps the coolest day visible.
const BAR_FLOOR: u64 = 15;
const BAR_CEILING: u64 = 100;

/// Every day at a stride that still looks like a bar chart. The caller uses
/// this to decide whether a side-by-side split can afford a chart at all.
pub(super) const COMFORTABLE_WIDTH: u16 = 22 * MIN_BAR_STRIDE - GAP;
/// Below this the bars stop being readable at all.
pub(super) const MIN_HEIGHT: u16 = 8;
/// Past this they grow spindly rather than informative.
pub(super) const MAX_HEIGHT: u16 = 16;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn palette() -> Palette {
        Theme::default().palette()
    }
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    /// Bars narrower than two cells read as a comb. This has regressed twice,
    /// once via the split threshold and once via the stride clamp.
    #[test]
    fn bars_never_fall_below_two_cells() {
        for width in 30u16..=250 {
            let columns = Columns::fit(width, 22, MIN_BAR_STRIDE, MAX_BAR_STRIDE);
            assert!(
                columns.stride - GAP >= 2,
                "width {width} gives {}-cell bars",
                columns.stride - GAP
            );
        }
    }

    #[test]
    fn renders_without_panicking_at_awkward_sizes() {
        let w = Weather::fixture(22, 14);
        for (width, height) in [(40, 12), (70, 30), (136, 24), (138, 20), (200, 50)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| chart_area_render(f, &w, palette(), f.area(), Unit::Imperial, 14))
                .unwrap();
        }
    }

    /// Column of the caret in the marker row, which is the chart's only
    /// non-colour statement about which bar is selected.
    fn caret_column(width: u16, height: u16, selected: usize) -> Option<u16> {
        let w = Weather::fixture(22, 14);
        let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
        t.draw(|f| chart_area_render(f, &w, palette(), f.area(), Unit::Imperial, selected))
            .unwrap();

        let buf = t.backend().buffer();
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .find(|&(x, y)| buf[(x, y)].symbol() == "^")
            .map(|(x, _)| x)
    }

    /// The finding: the bars carried the selection in yellow-versus-blue and
    /// nothing else, so a monochrome render lost it entirely.
    #[test]
    fn the_selected_bar_is_marked_without_colour() {
        assert!(
            caret_column(80, 14, 14).is_some(),
            "no marker under the selected bar"
        );
    }

    /// One caret per stride, in the same direction as the selection. Catching
    /// it pointing at the gap beside the bar, or drifting at a different rate,
    /// is the point.
    #[test]
    fn the_caret_tracks_the_selected_bar_one_stride_at_a_time() {
        let columns = Columns::fit(78, 22, MIN_BAR_STRIDE, MAX_BAR_STRIDE);
        let here = caret_column(80, 14, 14).expect("a marker");

        for step in 1..=3u16 {
            let moved = caret_column(80, 14, 14 + step as usize).expect("a marker");
            assert_eq!(
                moved,
                here + step * columns.stride,
                "the caret moved {} cells for {step} day(s), not {}",
                moved.saturating_sub(here),
                step * columns.stride
            );
        }
    }

    /// A narrow pane drops the oldest history, so a selection can sit off the
    /// left edge. Pointing at whatever bar happens to be leftmost would name
    /// the wrong day, which is worse than saying nothing.
    #[test]
    fn a_selection_outside_the_window_is_not_marked_at_all() {
        let narrow = 34u16;
        let columns = Columns::fit(narrow - 2, 22, MIN_BAR_STRIDE, MAX_BAR_STRIDE);
        assert!(
            columns.capacity < 22,
            "this test needs a pane too narrow for the whole window"
        );

        assert_eq!(
            caret_column(narrow, 12, 0),
            None,
            "the oldest day is off the window and must not be marked"
        );
    }

    /// The marker takes a row, so it must not take the last one a chart has.
    #[test]
    fn a_chart_too_short_for_a_marker_row_still_draws_bars() {
        let w = Weather::fixture(22, 14);
        for height in 3..=6u16 {
            let mut t = Terminal::new(TestBackend::new(80, height)).unwrap();
            t.draw(|f| chart_area_render(f, &w, palette(), f.area(), Unit::Imperial, 14))
                .unwrap();

            let buf = t.backend().buffer();
            let drew_a_bar = (0..80)
                .flat_map(|x| (0..height).map(move |y| (x, y)))
                .any(|(x, y)| matches!(buf[(x, y)].symbol(), "█" | "▄" | "▀" | "▆" | "▂"));
            assert!(drew_a_bar, "height {height} drew no bars at all");
        }
    }
}
