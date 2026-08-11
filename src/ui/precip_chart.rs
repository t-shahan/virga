//! The mirrored hourly chart: chance rises from a central rule, forecast
//! amount hangs below it.
//!
//! Showing both at once is the point. Amount alone is flat for days on end —
//! in a representative window only six hours of 173 carried any at all — and
//! chance alone hides whether it will come to anything. Together, a tall spike
//! with nothing under it reads as "might drizzle", and tall above *and* below
//! reads as "take the umbrella".
//!
//! `BarChart` cannot draw this: its `.direction()` picks horizontal versus
//! vertical bars, not up versus down. So this writes cells itself.

use crate::ui::bars::{Columns, GAP, window_start};
use crate::units::Unit;
use crate::weather::model::{HourlyForecast, Weather};
use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::Block;

/// The span the chart aims to show. Surplus width buys broader bars rather
/// than more hours: a day ahead is the decision-relevant window, and the full
/// eight-day series at a readable stride would need some 575 columns, so more
/// hours on screen just means thinner ones.
const TARGET_HOURS: usize = 24;
/// Two-cell bars at the narrow end, matching the daily chart; four at the wide
/// end, where the daily chart's three would leave the hourly bars looking
/// starved on a terminal with room to spare.
const MIN_STRIDE: u16 = 3;
const MAX_STRIDE: u16 = 5;

/// Bottom-anchored eighths, so a partial cell at the top of a rising bar fills
/// from below and meets the cell beneath it.
const RISING: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Falling bars need top-anchored partials, and only these three have
/// dependable font coverage. The eighth-resolution equivalents (U+1FB82–1FB86)
/// are Unicode 13 "Symbols for Legacy Computing" and render as tofu on stock
/// terminal fonts, so three levels it is. Amounts are usually zero or small,
/// which makes that a cheap trade.
const FALL_FULL: &str = "█";
const FALL_HALF: &str = "▀";
/// Present, but too little to round up to half a cell.
const FALL_TRACE: &str = "▔";

const RULE: &str = "─";
/// The selected hour, distinguished by shape as well as colour — the daily
/// chart encodes its selection in colour alone, which this should not repeat.
const RULE_SELECTED: &str = "═";
const RULE_NOW: &str = "┬";
/// Midnight, so the days are legible without spending a row on labels.
const RULE_MIDNIGHT: &str = "┼";

/// Rows the two halves and the rule need before the chart says anything.
const MIN_HEIGHT: u16 = 3 + 2;
/// Rows the amount half may take. It grows with the chart so a heavy hour has
/// somewhere to go, but it is near-empty most weeks and must not take the
/// chart over.
const MAX_FALL_ROWS: usize = 6;

/// Rows the plot itself may take. The box fills the terminal — a chart floating
/// above a band of dead space looks broken — but the plot inside it does not
/// stretch to match. Chance is scaled 0-100 and a typical week peaks under
/// half, so on a very tall terminal an unbounded plot puts twenty blank rows
/// above the bars. Capped and centred, the same window reads as a chart with
/// air around it.
const MAX_PLOT_HEIGHT: u16 = 16;

/// Below this the amount half is scaled as if this were the wettest hour on
/// record. Roughly moderate rainfall: without a floor, a week whose heaviest
/// hour is a 0.1 mm drizzle would draw that drizzle at full height.
const AMOUNT_FLOOR_MM: f64 = 2.0;

pub(super) fn precip_chart_render(
    frame: &mut Frame,
    weather: &Weather,
    area: Rect,
    unit: Unit,
    selected: usize,
) {
    let hours = weather.forecast_hours();
    let inner = Block::bordered().inner(area);

    if hours.is_empty() || inner.width == 0 || area.height < MIN_HEIGHT {
        frame.render_widget(Block::bordered().title("Precipitation"), area);
        return;
    }

    let columns = Columns::fit(inner.width, TARGET_HOURS, MIN_STRIDE, MAX_STRIDE);
    let shown = columns.capacity.min(hours.len());
    let start = window_start(selected, columns.capacity, hours.len());
    let visible = &hours[start..start + shown];

    // Scaled over the whole forward window rather than just what is on screen,
    // so the amount half keeps its meaning while you arrow along instead of
    // rescaling under you. The plan called for the visible maximum; a window
    // whose heaviest hour is a trace would then draw that trace full height.
    let heaviest = hours
        .iter()
        .filter_map(|h| h.precip_mm)
        .fold(0.0_f64, f64::max);
    let amount_scale = heaviest.max(AMOUNT_FLOOR_MM);

    let block = Block::bordered()
        .title(chart_title(shown, amount_scale, unit, inner.width))
        .title_bottom(Line::from(dry_spell(visible, shown)).dark_gray());
    frame.render_widget(block, area);

    // Centre the columns on their measured width, as the daily chart does,
    // then centre the plot within whatever height the box has been given.
    let [plot] = Layout::horizontal([Constraint::Length(columns.width_of(shown))])
        .flex(Flex::Center)
        .areas(inner);
    let [plot] = Layout::vertical([Constraint::Length(inner.height.min(MAX_PLOT_HEIGHT))])
        .flex(Flex::Center)
        .areas(plot);

    let rows = Rows::split(plot.height);

    for (i, hour) in visible.iter().enumerate() {
        let index = start + i;
        let colour = if index == selected {
            Color::Yellow
        } else if index == 0 {
            // Hour zero of the forward window is always now.
            Color::LightBlue
        } else {
            Color::Blue
        };

        let rising = rising_column(fraction(hour.chance.map(f64::from), 100.0), rows.rise);
        let falling = falling_column(fraction(hour.precip_mm, amount_scale), rows.fall);

        // The rule takes its column's colour except at a day boundary, which
        // is structural rather than a reading: the bars above it stay ordinary
        // while the mark itself takes the accent. Sharing the selection's
        // yellow costs little, because the selection is yellow down its whole
        // column and carries a different glyph.
        let (rule, rule_colour) = if index == selected {
            (RULE_SELECTED, colour)
        } else if index == 0 {
            (RULE_NOW, colour)
        } else if is_midnight(&hour.time) {
            (RULE_MIDNIGHT, Color::Yellow)
        } else {
            (RULE, colour)
        };

        let left = plot.x + (i * columns.stride as usize) as u16;
        let width = columns.stride - GAP;

        for x in left..(left + width).min(plot.right()) {
            for (offset, symbol) in rising.iter().enumerate() {
                put(frame, x, plot.y + offset as u16, symbol, colour);
            }
            put(frame, x, plot.y + rows.rise as u16, rule, rule_colour);
            for (offset, symbol) in falling.iter().enumerate() {
                let y = plot.y + (rows.rise + 1 + offset) as u16;
                put(frame, x, y, symbol, colour);
            }
        }
    }
}

/// How the interior rows are divided. The chance half takes the majority: it
/// is defined every hour and carries eight levels per cell, where the amount
/// half is usually empty and limited to three.
struct Rows {
    rise: usize,
    fall: usize,
}

impl Rows {
    fn split(height: u16) -> Self {
        let interior = height as usize;
        let fall = (interior / 4).clamp(1, MAX_FALL_ROWS);
        Self {
            rise: interior.saturating_sub(fall + 1),
            fall,
        }
    }
}

fn put(frame: &mut Frame, x: u16, y: u16, symbol: &str, colour: Color) {
    if symbol == " " {
        return;
    }
    let area = frame.area();
    if x >= area.right() || y >= area.bottom() {
        return;
    }
    frame.buffer_mut()[(x, y)]
        .set_symbol(symbol)
        .set_style(Style::new().fg(colour));
}

/// `value` as a share of `scale`, clamped, with a missing reading reading as
/// nothing rather than as zero-with-confidence.
fn fraction(value: Option<f64>, scale: f64) -> f64 {
    match value {
        Some(v) if scale > 0.0 => (v / scale).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

/// A rising bar, top row first. Eighth resolution, so a 3% chance still leaves
/// a visible mark instead of rounding away.
fn rising_column(fraction: f64, rows: usize) -> Vec<&'static str> {
    let eighths = (fraction * (rows * 8) as f64).round() as usize;

    (0..rows)
        .map(|row| {
            let below = (rows - 1 - row) * 8;
            RISING[eighths.saturating_sub(below).min(8)]
        })
        .collect()
}

/// A falling bar, top row first — the row nearest the rule comes first, since
/// that is where a falling bar starts.
fn falling_column(fraction: f64, rows: usize) -> Vec<&'static str> {
    let mut cells = vec![" "; rows];
    if fraction <= 0.0 {
        return cells;
    }

    let halves = (fraction * (rows * 2) as f64).round() as usize;
    if halves == 0 {
        // Positive but too small to round up. Saying so with the thinnest mark
        // available beats drawing nothing, which would read as a dry hour.
        cells[0] = FALL_TRACE;
        return cells;
    }

    for (row, cell) in cells.iter_mut().enumerate() {
        *cell = match halves.saturating_sub(row * 2) {
            0 => break,
            1 => FALL_HALF,
            _ => FALL_FULL,
        };
    }
    cells
}

/// Longest form that fits. The span matters most — it is what the arrows move
/// through — then the fact that the halves are different quantities, then the
/// amount scale that stops the two inviting a meaningless comparison.
fn chart_title(hours: usize, scale_mm: f64, unit: Unit, width: u16) -> String {
    let span = format!("Precipitation · next {hours} h");
    let legend = format!("{span} · chance ▲ · amount ▼");
    let full = format!(
        "{legend} 0–{:.*} {}",
        unit.precip_decimals(),
        unit.precip(scale_mm),
        unit.precip_label()
    );

    // Two corners plus a column of breathing room inside each, as the current
    // pane's border budget does.
    let room = width.saturating_sub(2) as usize;
    for candidate in [full, legend, span] {
        if candidate.chars().count() <= room {
            return candidate;
        }
    }
    "Precipitation".to_string()
}

/// With most hours dry most weeks, an empty lower half is the screen's normal
/// state rather than an edge case. Say so in words, or the chart looks broken
/// rather than reassuring.
fn dry_spell(visible: &[HourlyForecast], hours: usize) -> String {
    if visible.iter().any(HourlyForecast::is_wet) {
        return String::new();
    }
    format!("no rain or snow in the next {hours} h")
}

fn is_midnight(time: &str) -> bool {
    time.get(11..13) == Some("00")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn a_rising_bar_fills_from_the_bottom_up() {
        // Full height: every cell solid.
        assert_eq!(rising_column(1.0, 3), ["█", "█", "█"]);
        // Two thirds: the top cell is empty, the rest solid.
        assert_eq!(rising_column(2.0 / 3.0, 3), [" ", "█", "█"]);
        // Nothing at all.
        assert_eq!(rising_column(0.0, 3), [" ", " ", " "]);
    }

    /// A few percent must still leave a mark. Eighth resolution is what buys
    /// that, and it is why the rising half does not need the amount half's
    /// trace glyph.
    #[test]
    fn a_small_chance_still_shows() {
        let column = rising_column(0.05, 3);
        assert_eq!(column[2], "▁", "5% leaves the thinnest mark: {column:?}");
        assert!(column[0] == " " && column[1] == " ");
    }

    #[test]
    fn a_falling_bar_hangs_from_the_rule_down() {
        assert_eq!(falling_column(1.0, 2), ["█", "█"]);
        assert_eq!(falling_column(0.5, 2), ["█", " "]);
        assert_eq!(falling_column(0.25, 2), ["▀", " "]);
        assert_eq!(falling_column(0.0, 2), [" ", " "]);
    }

    /// A positive amount too small to round up must not render as a dry hour.
    /// A 0.1 mm hour against a 2 mm scale is exactly this case, and it is the
    /// common one — most rain in the sample window was precisely 0.1 mm.
    #[test]
    fn a_trace_amount_is_marked_rather_than_dropped() {
        let column = falling_column(0.1 / AMOUNT_FLOOR_MM, 2);
        assert_eq!(column[0], FALL_TRACE, "got {column:?}");
        assert_ne!(column[0], " ", "a trace is not nothing");
    }

    /// Only these three glyphs may appear below the rule. The eighth-resolution
    /// alternatives render as boxes on stock fonts, which is why the downward
    /// half is deliberately coarse.
    #[test]
    fn falling_bars_use_only_well_supported_glyphs() {
        for step in 0..=40 {
            for rows in 1..=4 {
                for cell in falling_column(f64::from(step) / 40.0, rows) {
                    assert!(
                        [" ", FALL_FULL, FALL_HALF, FALL_TRACE].contains(&cell),
                        "{cell:?} is not a safe downward glyph"
                    );
                }
            }
        }
    }

    /// A missing reading is not a zero reading, but neither may it panic or
    /// draw a full-height bar.
    #[test]
    fn a_missing_reading_draws_nothing() {
        assert_eq!(fraction(None, 100.0), 0.0);
        assert_eq!(fraction(Some(50.0), 0.0), 0.0, "no scale, no bar");
        assert_eq!(
            fraction(Some(500.0), 100.0),
            1.0,
            "clamped, not overflowing"
        );
    }

    #[test]
    fn the_rows_always_add_up_to_the_height() {
        for height in 3u16..=40 {
            let rows = Rows::split(height);
            assert_eq!(
                rows.rise + rows.fall + 1,
                height as usize,
                "height {height} lost a row"
            );
            assert!(rows.fall >= 1, "height {height} left no room to fall");
            assert!(
                rows.fall <= MAX_FALL_ROWS,
                "height {height} let the amount half take over"
            );
            assert!(
                rows.rise >= rows.fall,
                "height {height}: chance should keep the majority"
            );
        }
    }

    fn buffer_text(width: u16, height: u16, selected: usize) -> String {
        let weather = Weather::fixture(22, 14);
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, selected))
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Assert against what was actually drawn, not against what the arithmetic
    /// should have produced.
    /// The rule is the one row drawn in every column, so it carries the now,
    /// selected and midnight markers. A window wide enough to hold all three
    /// is what proves they coexist rather than overwriting each other.
    #[test]
    fn the_rule_carries_now_the_selection_and_midnight_at_once() {
        // Wide enough to span a day boundary: the bars are broader now, so a
        // screenful is fewer hours than it used to be.
        let text = buffer_text(120, 10, 5);
        let rule_row = text
            .lines()
            .find(|line| line.contains(RULE_SELECTED))
            .unwrap_or_else(|| panic!("no rule row drawn in:\n{text}"));

        assert!(rule_row.contains(RULE), "the plain rule spans the rest");
        assert!(rule_row.contains(RULE_NOW), "now is marked: {rule_row:?}");
        assert!(
            rule_row.contains(RULE_MIDNIGHT),
            "midnight is marked: {rule_row:?}"
        );
    }

    #[test]
    fn the_selected_hour_is_marked_by_shape_not_only_colour() {
        let text = buffer_text(100, 10, 5);
        assert!(
            text.contains(RULE_SELECTED),
            "the selection needs a non-colour identity:\n{text}"
        );
        assert!(
            text.contains(RULE_NOW),
            "and now stays distinct from it:\n{text}"
        );
    }

    /// Three states, not two — the daily chart had a real bug from conflating
    /// "today" with "selected".
    #[test]
    fn now_and_the_selection_stay_distinguishable() {
        let text = buffer_text(60, 10, 0);
        assert!(
            text.contains(RULE_SELECTED),
            "with the selection on now, the selection wins:\n{text}"
        );
    }

    #[test]
    fn a_dry_window_says_so_in_words() {
        let mut weather = Weather::fixture(22, 14);
        for hour in &mut weather.hourly {
            hour.precip_mm = Some(0.0);
        }

        let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
        terminal
            .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, 0))
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let bottom: String = (0..60).map(|x| buffer[(x, 9)].symbol()).collect();
        assert!(
            bottom.contains("no rain or snow"),
            "a dry week should still say something: {bottom:?}"
        );
    }

    #[test]
    fn a_wet_window_drops_the_dry_caption() {
        let text = buffer_text(60, 10, 0);
        assert!(!text.contains("no rain or snow"), "\n{text}");
    }

    /// The scale belongs in the title or the two halves invite a comparison
    /// that means nothing — they are percentages against inches.
    #[test]
    fn the_title_names_the_amount_scale_when_it_fits() {
        let title = chart_title(26, 2.0, Unit::Imperial, 78);
        assert!(title.contains("chance ▲"), "{title}");
        assert!(title.contains("amount ▼"), "{title}");
        assert!(title.contains("in"), "{title}");
        assert!(title.contains("next 26 h"), "{title}");
    }

    /// The span survives longest, because it is what the arrows move through.
    #[test]
    fn the_title_sheds_detail_rather_than_overflowing() {
        for width in 4u16..=120 {
            let title = chart_title(26, 2.0, Unit::Metric, width);
            assert!(
                title.chars().count()
                    <= (width.saturating_sub(2) as usize).max("Precipitation".len()),
                "width {width}: {title:?} does not fit"
            );
        }
        assert_eq!(chart_title(26, 2.0, Unit::Metric, 10), "Precipitation");
        assert!(chart_title(26, 2.0, Unit::Metric, 34).contains("next 26 h"));
    }

    #[test]
    fn midnight_is_recognised_from_the_stamp() {
        assert!(is_midnight("2026-08-10T00:00"));
        assert!(!is_midnight("2026-08-10T01:00"));
        assert!(!is_midnight("nonsense"));
    }

    /// Including the app's declared minimum, and either side of the height at
    /// which the chart gives up on drawing halves at all.
    #[test]
    fn renders_without_panicking_at_awkward_sizes() {
        let weather = Weather::fixture(22, 14);
        for (width, height) in [
            (34, 4),
            (34, 5),
            (34, 6),
            (40, 12),
            (60, 8),
            (80, 10),
            (200, 50),
            (3, 3),
            (1, 1),
        ] {
            for selected in [0usize, 1, 95, 191] {
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, selected))
                    .unwrap();
            }
        }
    }

    /// An empty hourly series is a real response shape — a stripped request, or
    /// a location the model does not cover.
    #[test]
    fn an_empty_series_renders_an_empty_box() {
        let mut weather = Weather::fixture(22, 14);
        weather.hourly.clear();
        weather.now_hour = 0;

        let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
        terminal
            .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, 0))
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let top: String = (0..60).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(top.contains("Precipitation"), "{top:?}");
    }

    /// `put` deliberately skips blank cells, which is only safe because
    /// ratatui resets the buffer it is about to hand back. If that ever stops
    /// being true, a redraw would keep glyphs from the frame before it — and
    /// the chart scrolls, so almost every cell changes between frames.
    #[test]
    fn redrawing_leaves_nothing_behind_from_the_previous_frame() {
        let weather = Weather::fixture(22, 14);
        let mut reused = Terminal::new(TestBackend::new(90, 12)).unwrap();

        for selected in [0usize, 40, 7, 191, 12] {
            reused
                .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, selected))
                .unwrap();

            let mut fresh = Terminal::new(TestBackend::new(90, 12)).unwrap();
            fresh
                .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, selected))
                .unwrap();

            assert_eq!(
                reused.backend().buffer(),
                fresh.backend().buffer(),
                "selection {selected} kept cells from the frame before it"
            );
        }
    }

    /// The box fills the terminal so nothing floats above dead space, but the
    /// plot inside it stays proportionate however tall the window gets.
    #[test]
    fn a_tall_terminal_fills_its_box_without_stretching_the_plot() {
        let weather = Weather::fixture(22, 14);

        for height in [8u16, 12, 18, 24, 44, 60] {
            let mut terminal = Terminal::new(TestBackend::new(90, height)).unwrap();
            terminal
                .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, 3))
                .unwrap();
            let buffer = terminal.backend().buffer().clone();

            let rows: Vec<String> = (0..height)
                .map(|y| (0..90).map(|x| buffer[(x, y)].symbol()).collect())
                .collect();

            // The border reaches the last row, so the box fills its area.
            assert!(
                rows[height as usize - 1].contains('└'),
                "height {height}: the box did not reach the bottom:\n{}",
                rows.join("\n")
            );

            // Rows the plot drew on. Matching the plain rule would catch the
            // border too — it is the same glyph — so look for marks only the
            // plot makes.
            let plotted = |row: &String| {
                row.chars().any(|c| {
                    RISING[1..].contains(&c.to_string().as_str())
                        || [
                            FALL_FULL,
                            FALL_HALF,
                            FALL_TRACE,
                            RULE_SELECTED,
                            RULE_NOW,
                            RULE_MIDNIGHT,
                        ]
                        .contains(&c.to_string().as_str())
                })
            };
            let drawn: Vec<usize> = (0..height as usize)
                .filter(|&y| plotted(&rows[y]))
                .collect();
            let first = *drawn.first().expect("something was plotted");
            let last = *drawn.last().expect("something was plotted");

            assert!(
                last - first < MAX_PLOT_HEIGHT as usize,
                "height {height}: the plot spans {} rows:\n{}",
                last - first + 1,
                rows.join("\n")
            );
            // And it never runs into the border it sits inside.
            assert!(first >= 1 && last <= height as usize - 2, "height {height}");
        }
    }

    /// The bottom border must land inside the area, not one cell past it — a
    /// pty reconstruction made it look like it wrapped.
    #[test]
    fn the_box_closes_inside_its_own_width() {
        let weather = Weather::fixture(22, 14);
        for (w, h) in [(118u16, 22u16), (118, 26), (90, 12), (119, 20), (37, 14)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, 3))
                .unwrap();
            let b = t.backend().buffer().clone();
            let bottom: String = (0..w).map(|x| b[(x, h - 1)].symbol()).collect();
            assert!(bottom.starts_with('└'), "{w}x{h}: {bottom:?}");
            assert!(bottom.ends_with('┘'), "{w}x{h}: {bottom:?}");
            assert_eq!(bottom.chars().count(), w as usize, "{w}x{h}");
        }
    }

    /// Day boundaries are landmarks rather than readings, so the mark takes the
    /// accent while the bars above it stay ordinary.
    #[test]
    fn midnight_is_marked_in_the_accent_colour() {
        let weather = Weather::fixture(22, 14);
        let mut terminal = Terminal::new(TestBackend::new(120, 10)).unwrap();
        terminal
            .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, 5))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        let midnight = (0..120u16)
            .flat_map(|x| (0..10u16).map(move |y| (x, y)))
            .find(|&(x, y)| buffer[(x, y)].symbol() == RULE_MIDNIGHT)
            .expect("a midnight rule cell");

        assert_eq!(buffer[midnight].style().fg, Some(Color::Yellow));

        // The column it marks is still an ordinary one above the rule.
        let (x, y) = midnight;
        assert_eq!(
            buffer[(x, y - 1)].style().fg,
            Some(Color::Blue),
            "only the boundary itself takes the accent"
        );
    }

    /// Colour reinforces the three states but must never be the only carrier.
    #[test]
    fn the_selection_is_coloured_as_well_as_shaped() {
        let weather = Weather::fixture(22, 14);
        let mut terminal = Terminal::new(TestBackend::new(60, 10)).unwrap();
        terminal
            .draw(|f| precip_chart_render(f, &weather, f.area(), Unit::Imperial, 5))
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let selected = (0..60)
            .flat_map(|x| (0..10).map(move |y| (x, y)))
            .find(|&(x, y)| buffer[(x, y)].symbol() == RULE_SELECTED)
            .expect("a selected rule cell");

        assert_eq!(
            buffer[selected].style().fg,
            Some(Color::Yellow),
            "the loud colour goes to the selection"
        );
    }
}
