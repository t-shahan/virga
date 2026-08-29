//! The weathergram: four tracks of the coming hours on one clock.
//!
//! The full layout is a skyline. Temperature gets four rows of filled
//! silhouette, so a day has a shape rather than a row of five block heights,
//! and rain gets a two-row band under it on the same floor. Ink is reserved
//! for information: the sky row is a faint rail carrying the condition's
//! emoji every third hour, the wind row draws an arrow every second hour
//! with its speed on the six-hour ticks, and a dry hour in the rain band
//! draws nothing at all. An earlier draft gave every hour one centred glyph
//! per track, which spent most of the plot restating that nothing was
//! changing and read as noise at 48 columns.
//!
//! The compact layout keeps the one-row-per-track form: below nineteen rows
//! there is no height for a silhouette, and a single glyph per hour is the
//! densest honest rendering left.

use crate::theme::Palette;
use crate::ui::axis::{hour_ticks_render, put, put_right, put_styled, put_text};
use crate::ui::bars::window_start;
use crate::ui::condition_symbol;
use crate::ui::precipitation::{PrecipitationAggregate, aggregate};
use crate::units::Unit;
use crate::weather::model::HourlyForecast;
use chrono::{NaiveDateTime, Timelike};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::Block;

/// Rows the temperature silhouette stands in.
const TEMP_ROWS: u16 = 4;
/// Rows the rain band stands in.
const RAIN_ROWS: u16 = 2;

/// The full interior, top to bottom: sky, the temperature silhouette, the
/// rain band, wind, the clock axis, and the selection marker.
const SKY_ROW: u16 = 0;
const TEMP_ROW: u16 = 1;
const RAIN_ROW: u16 = TEMP_ROW + TEMP_ROWS;
const WIND_ROW: u16 = RAIN_ROW + RAIN_ROWS;
const AXIS_ROW: u16 = WIND_ROW + 1;
const MARKER_ROW: u16 = AXIS_ROW + 1;

pub(super) const FULL_ROWS: u16 = MARKER_ROW + 1 + 2;
pub(super) const COMPACT_ROWS: u16 = 6;

const BORDER_COLS: u16 = 2;
const LABEL_WIDTH: u16 = 6;
const SUMMARY_GAP: u16 = 1;
const SUMMARY_WIDTH: u16 = 12;
const HORIZONS: [usize; 3] = [48, 36, 24];
const COMPACT_HORIZON: usize = 12;
const MAX_CELL_WIDTH: u16 = 3;

/// Every eighth from a floor line to a full block: the vertical resolution
/// both bands draw with.
const RAMP: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Below this chance the rain band draws dimmed, matching the week strip's
/// faint low steps: present, but not competing with hours that would change a
/// plan.
const FAINT_BELOW: u8 = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Window {
    start: usize,
    hours: usize,
    cell_width: u16,
}

fn window_for(width: u16, selected: usize, count: usize) -> Window {
    let plot_width = width.saturating_sub(BORDER_COLS + LABEL_WIDTH + SUMMARY_GAP + SUMMARY_WIDTH);
    let horizon = HORIZONS
        .into_iter()
        .find(|hours| plot_width as usize >= hours * 2)
        .unwrap_or(COMPACT_HORIZON);
    let hours = if count == 0 {
        horizon
    } else {
        horizon.min(count)
    };
    let cell_width = (plot_width / hours as u16).clamp(1, MAX_CELL_WIDTH);

    Window {
        start: window_start(selected.min(count.saturating_sub(1)), hours, count),
        hours,
        cell_width,
    }
}

fn temperature_range(hours: &[HourlyForecast]) -> Option<(f64, f64)> {
    let mut temperatures = hours.iter().filter_map(|hour| hour.temp_c);
    let first = temperatures.next()?;
    Some(temperatures.fold((first, first), |(min, max), value| {
        (min.min(value), max.max(value))
    }))
}

/// Column height in eighths for the silhouette, `1..=32` between the visible
/// extremes. The coldest visible hour keeps one line of ink, so a present
/// reading is never confused with a missing one, and a flat window stands at
/// half height rather than vanishing or filling the band.
fn temperature_eighths(value: Option<f64>, range: Option<(f64, f64)>) -> Option<u16> {
    let levels = TEMP_ROWS * 8;
    let (Some(value), Some((min, max))) = (value, range) else {
        return None;
    };
    if min == max {
        return Some(levels / 2);
    }

    let scaled = ((value - min) / (max - min)).clamp(0.0, 1.0) * f64::from(levels - 1);
    Some(1 + scaled.round() as u16)
}

/// Band height in sixteenths for the chance of rain, or `None` where the band
/// stays dark. Below ten percent is deliberately no ink at all: the old ramp
/// drew a dot for every dry hour, and a dry week became a dotted line saying
/// nothing 48 times. The cost is that a forecast of "under 10%" and no
/// forecast at all look alike here; the summary still tells them apart, and
/// the inspector states the selected hour exactly.
fn rain_sixteenths(chance: Option<u8>) -> Option<u16> {
    let chance = chance.filter(|chance| *chance >= 10)?;
    Some(
        ((f64::from(chance) / 100.0) * f64::from(RAIN_ROWS * 8))
            .round()
            .max(1.0) as u16,
    )
}

/// The glyph one band row shows for a column `height` eighths tall: full
/// blocks below the surface, a partial at the surface, nothing above it.
fn band_glyph(height: u16, rows_above_floor: u16) -> Option<&'static str> {
    let filled = height.saturating_sub(rows_above_floor * 8).min(8);
    (filled > 0).then(|| RAMP[filled as usize - 1])
}

/// One bottom-anchored column of a band.
fn band_render(frame: &mut Frame, x: u16, top_y: u16, rows: u16, height: u16, style: Style) {
    for above_floor in 0..rows {
        let Some(glyph) = band_glyph(height, above_floor) else {
            continue;
        };
        put_styled(frame, x, top_y + (rows - 1 - above_floor), glyph, style);
    }
}

/// The five-step silhouette the compact layout keeps: one row leaves no room
/// for a band, so height quantised to the glyph is the whole vocabulary.
fn temperature_step(value: Option<f64>, range: Option<(f64, f64)>) -> &'static str {
    const STEPS: [&str; 5] = ["▁", "▂", "▄", "▆", "█"];

    let (Some(value), Some((min, max))) = (value, range) else {
        return " ";
    };
    if min == max {
        return "▄";
    }

    let step = (((value - min) / (max - min)).clamp(0.0, 1.0) * 4.0).round() as usize;
    STEPS[step]
}

fn rain_step(chance: Option<u8>) -> &'static str {
    match chance {
        None => " ",
        Some(0..=9) => "·",
        Some(10..=29) => "▂",
        Some(30..=49) => "▄",
        Some(50..=69) => "▆",
        Some(_) => "█",
    }
}

fn wind_symbol(speed_kph: Option<f64>, direction: Option<f64>) -> &'static str {
    const ARROWS: [&str; 8] = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"];
    let Some(speed_kph) = speed_kph else {
        return " ";
    };
    if speed_kph < 1.0 {
        return "·";
    }
    let Some(degrees) = direction else {
        return " ";
    };
    let normalized = degrees.rem_euclid(360.0);
    ARROWS[((normalized / 45.0).round() as usize) % ARROWS.len()]
}

/// The clock hour of a forecast timestamp, for the sky and wind cadences. An
/// unparseable stamp reports `None` and each row falls back to a cadence
/// counted from the window edge rather than hiding data behind a broken
/// clock.
fn clock_hour(time: &str) -> Option<u32> {
    NaiveDateTime::parse_from_str(time, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|at| at.hour())
}

fn temperature_summary(hours: &[HourlyForecast], unit: Unit) -> String {
    temperature_range(hours).map_or_else(
        || "—".to_string(),
        |(low, high)| {
            format!(
                "{:.0}–{:.0}{}",
                unit.temp(low),
                unit.temp(high),
                unit.temp_symbol()
            )
        },
    )
}

fn precipitation_summary(hours: &[HourlyForecast], selected: usize, unit: Unit) -> String {
    let end = selected.saturating_add(24).min(hours.len());
    let window = hours.get(selected..end).unwrap_or_default();
    let total = aggregate(window, unit);
    match total {
        PrecipitationAggregate::Unavailable => "—".to_string(),
        PrecipitationAggregate::Zero => format!("0 {}", unit.precip_label()),
        PrecipitationAggregate::Trace(_) | PrecipitationAggregate::Measured(_) => total
            .positive_text(unit, " ")
            .expect("positive aggregate has text"),
    }
}

fn wind_summary(hours: &[HourlyForecast], unit: Unit) -> String {
    let mut speeds = hours.iter().filter_map(|hour| hour.wind_kph);
    let Some(first) = speeds.next() else {
        return "—".to_string();
    };
    let (minimum, maximum) = speeds.fold((first, first), |(minimum, maximum), speed| {
        (minimum.min(speed), maximum.max(speed))
    });
    format!(
        "{:.0}–{:.0} {}",
        unit.speed(minimum),
        unit.speed(maximum),
        unit.speed_label()
    )
}

/// Everything a track needs to place itself: the visible window and the
/// measured columns the frame gave it.
struct Plot {
    window: Window,
    content_x: u16,
    plot_x: u16,
    plot_width: u16,
    summary_x: u16,
}

/// The colour a column's state demands, or `None` for an ordinary hour that
/// takes its track's own colour.
fn state_colour(palette: Palette, index: usize, selected: usize) -> Option<ratatui::style::Color> {
    if index == selected {
        Some(palette.selection)
    } else if index == 0 {
        Some(palette.now)
    } else {
        None
    }
}

pub(super) fn weathergram_render(
    frame: &mut Frame,
    hours: &[HourlyForecast],
    palette: Palette,
    area: Rect,
    unit: Unit,
    selected: usize,
    compact: bool,
) {
    let window = window_for(area.width, selected, hours.len());
    let end = window.start.saturating_add(window.hours).min(hours.len());
    let visible = hours.get(window.start..end).unwrap_or_default();
    let block = Block::bordered().border_style(Style::new().fg(palette.border));
    let block = if compact {
        block
    } else {
        block.title(
            Line::from(format!(" Hourly weather · next {} h ", window.hours)).fg(palette.muted),
        )
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let plot_width = window.hours as u16 * window.cell_width;
    let used_width = LABEL_WIDTH + plot_width + SUMMARY_GAP + SUMMARY_WIDTH;
    let content_x = inner.x + inner.width.saturating_sub(used_width) / 2;
    let plot = Plot {
        window,
        content_x,
        plot_x: content_x + LABEL_WIDTH,
        plot_width,
        summary_x: content_x + LABEL_WIDTH + plot_width + SUMMARY_GAP,
    };

    if compact {
        compact_tracks_render(
            frame, hours, visible, palette, unit, selected, &plot, area, inner,
        );
    } else {
        full_tracks_render(frame, hours, visible, palette, unit, selected, &plot, inner);
    }
}

#[allow(clippy::too_many_arguments)]
fn full_tracks_render(
    frame: &mut Frame,
    hours: &[HourlyForecast],
    visible: &[HourlyForecast],
    palette: Palette,
    unit: Unit,
    selected: usize,
    plot: &Plot,
    inner: Rect,
) {
    let window = plot.window;
    let sky_y = inner.y + SKY_ROW;
    let temp_y = inner.y + TEMP_ROW;
    let rain_y = inner.y + RAIN_ROW;
    let wind_y = inner.y + WIND_ROW;
    let axis_y = inner.y + AXIS_ROW;
    let marker_y = inner.y + MARKER_ROW;

    // A band's label and summary sit low in the band, where its ink gathers.
    let temp_label_y = temp_y + TEMP_ROWS / 2;
    let rain_label_y = rain_y + RAIN_ROWS - 1;

    for (label, summary, y) in [
        ("sky", String::new(), sky_y),
        ("temp", temperature_summary(visible, unit), temp_label_y),
        (
            "rain",
            precipitation_summary(hours, selected, unit),
            rain_label_y,
        ),
        ("wind", wind_summary(visible, unit), wind_y),
    ] {
        put_text(frame, plot.content_x, y, label, palette.muted);
        put_right(
            frame,
            plot.summary_x,
            y,
            SUMMARY_WIDTH,
            &summary,
            palette.muted,
        );
    }

    let range = temperature_range(visible);
    let rail = Style::new().fg(palette.muted).add_modifier(Modifier::DIM);

    for (offset, hour) in visible.iter().enumerate() {
        let index = window.start + offset;
        let x0 = plot.plot_x + offset as u16 * window.cell_width;
        let centre = x0 + (window.cell_width - 1) / 2;
        let state = state_colour(palette, index, selected);
        let clock = clock_hour(&hour.time);

        // Sky: the condition's emoji every third clock hour, on a faint rail.
        // The steady cadence is what keeps the row from reading as scattered
        // leftovers, and a two-cell emoji spans its two-cell hour column
        // exactly, so the symbols sit centred where one-cell glyphs never
        // could. The rail still says the condition is known between marks,
        // and a gap still says the provider made no claim. An hour column too
        // narrow for a wide glyph falls back to the one-cell text symbol.
        let known = condition_symbol::symbol(hour.code) != " ";
        let cadence = clock.map_or(offset % 3 == 0, |hour| hour % 3 == 0);
        let emoji_cells = if cadence && known && window.cell_width >= 2 {
            put(
                frame,
                x0,
                sky_y,
                condition_symbol::emoji(hour.code),
                state.unwrap_or(palette.text),
            );
            2
        } else if cadence && known {
            put(
                frame,
                centre,
                sky_y,
                condition_symbol::symbol(hour.code),
                state.unwrap_or(palette.text),
            );
            1
        } else {
            0
        };
        if known {
            for column in emoji_cells..window.cell_width {
                put_styled(frame, x0 + column, sky_y, "─", rail);
            }
        }

        // Temperature: every cell of the hour inked, so the silhouette is
        // continuous rather than a picket fence of centred glyphs.
        if let Some(height) = temperature_eighths(hour.temp_c, range) {
            let style = Style::new().fg(state.unwrap_or(palette.accent));
            for column in 0..window.cell_width {
                band_render(frame, x0 + column, temp_y, TEMP_ROWS, height, style);
            }
        }

        // Rain: the same floor. Dim below the week strip's faint threshold,
        // except for the selection and the current hour, which are states
        // rather than readings and draw at full strength.
        if let Some(height) = rain_sixteenths(hour.chance) {
            let mut style = Style::new().fg(state.unwrap_or(palette.accent));
            if state.is_none() && hour.chance.is_some_and(|chance| chance < FAINT_BELOW) {
                style = style.add_modifier(Modifier::DIM);
            }
            for column in 0..window.cell_width {
                band_render(frame, x0 + column, rain_y, RAIN_ROWS, height, style);
            }
        }

        // Wind: an arrow every second hour, its speed on the six-hour ticks.
        // Hour after hour of near-identical arrows says less than a sparse
        // row the eye can actually compare; the selected hour always draws so
        // the marker below never points at a blank.
        if clock.map_or(offset % 2 == 0, |hour| hour % 2 == 0) || index == selected {
            let arrow = wind_symbol(hour.wind_kph, hour.wind_dir_deg);
            put(frame, centre, wind_y, arrow, state.unwrap_or(palette.text));

            // Not beside the selection: the selected hour's off-cadence arrow
            // would overprint the tick's digits, and the inspector already
            // carries that hour's exact speed.
            if arrow != " "
                && clock.is_some_and(|hour| hour % 6 == 0)
                && selected.abs_diff(index) > 1
                && let Some(speed) = hour.wind_kph
            {
                let text = format!("{:.0}", unit.speed(speed));
                if centre + 1 + text.chars().count() as u16 <= plot.plot_x + plot.plot_width {
                    put_text(frame, centre + 1, wind_y, &text, palette.muted);
                }
            }
        }
    }

    // The clock reads at the bottom, under the columns it measures, where a
    // meteogram keeps it.
    hour_ticks_render(
        frame,
        Rect::new(plot.plot_x, axis_y, plot.plot_width, 1),
        visible.iter().map(|hour| hour.time.as_str()),
        window.cell_width,
        0,
        if window.start == 0 {
            palette.now
        } else {
            palette.muted
        },
        palette,
    );

    let end = window.start + visible.len();
    if selected >= window.start && selected < end {
        let offset = selected - window.start;
        let x = plot.plot_x + offset as u16 * window.cell_width + (window.cell_width - 1) / 2;
        put(frame, x, marker_y, "▲", palette.selection);
    }
}

#[allow(clippy::too_many_arguments)]
fn compact_tracks_render(
    frame: &mut Frame,
    hours: &[HourlyForecast],
    visible: &[HourlyForecast],
    palette: Palette,
    unit: Unit,
    selected: usize,
    plot: &Plot,
    area: Rect,
    inner: Rect,
) {
    let window = plot.window;
    let axis_y = area.y;
    let first_track_y = inner.y;
    let marker_y = area.bottom().saturating_sub(1);

    put_text(frame, area.x + 1, area.y, "Hourly", palette.muted);
    hour_ticks_render(
        frame,
        Rect::new(plot.plot_x, axis_y, plot.plot_width, 1),
        visible.iter().map(|hour| hour.time.as_str()),
        window.cell_width,
        1,
        if window.start == 0 {
            palette.now
        } else {
            palette.muted
        },
        palette,
    );

    let range = temperature_range(visible);
    let summaries = [
        String::new(),
        temperature_summary(visible, unit),
        precipitation_summary(hours, selected, unit),
        wind_summary(visible, unit),
    ];

    for (row, (label, summary)) in ["sky", "temp", "rain", "wind"]
        .into_iter()
        .zip(summaries)
        .enumerate()
    {
        let y = first_track_y + row as u16;
        put_text(frame, plot.content_x, y, label, palette.muted);
        put_right(
            frame,
            plot.summary_x,
            y,
            SUMMARY_WIDTH,
            &summary,
            palette.muted,
        );

        for (offset, hour) in visible.iter().enumerate() {
            let index = window.start + offset;
            let symbol = match row {
                0 => condition_symbol::symbol(hour.code),
                1 => temperature_step(hour.temp_c, range),
                2 => rain_step(hour.chance),
                _ => wind_symbol(hour.wind_kph, hour.wind_dir_deg),
            };
            let colour =
                state_colour(palette, index, selected).unwrap_or(if matches!(row, 1 | 2) {
                    palette.accent
                } else {
                    palette.text
                });
            let x = plot.plot_x + offset as u16 * window.cell_width + (window.cell_width - 1) / 2;
            put(frame, x, y, symbol, colour);
        }
    }

    let end = window.start + visible.len();
    if selected >= window.start && selected < end {
        let offset = selected - window.start;
        let x = plot.plot_x + offset as u16 * window.cell_width + (window.cell_width - 1) / 2;
        put(frame, x, marker_y, "▲", palette.selection);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Color;

    /// Buffer rows the full layout's tracks land on, borders included.
    const FULL_SKY_Y: u16 = 1 + SKY_ROW;
    const FULL_TEMP_LABEL_Y: u16 = 1 + TEMP_ROW + TEMP_ROWS / 2;
    const FULL_TEMP_FLOOR_Y: u16 = 1 + TEMP_ROW + TEMP_ROWS - 1;
    const FULL_WIND_Y: u16 = 1 + WIND_ROW;
    const FULL_AXIS_Y: u16 = 1 + AXIS_ROW;

    #[test]
    fn horizons_are_quantized_from_measured_plot_width() {
        assert_eq!(window_for(34, 0, 192).hours, 12);
        assert_eq!(window_for(80, 0, 192).hours, 24);
        assert_eq!(window_for(100, 0, 192).hours, 36);
        assert_eq!(window_for(120, 0, 192).hours, 48);
    }

    #[test]
    fn a_selection_moves_inside_a_stable_page() {
        for selected in 0..24 {
            assert_eq!(window_for(80, selected, 192).start, 0);
        }
        assert_eq!(window_for(80, 24, 192).start, 24);
        assert_eq!(window_for(80, 191, 192).start, 168);
    }

    #[test]
    fn the_silhouette_scales_between_the_visible_extremes() {
        let range = Some((10.0, 20.0));
        assert_eq!(temperature_eighths(Some(10.0), range), Some(1));
        assert_eq!(temperature_eighths(Some(20.0), range), Some(32));
        assert_eq!(temperature_eighths(Some(15.0), range), Some(17));
        assert_eq!(
            temperature_eighths(Some(12.0), Some((12.0, 12.0))),
            Some(16)
        );
        assert_eq!(temperature_eighths(None, range), None);
        assert_eq!(temperature_eighths(Some(10.0), None), None);
    }

    #[test]
    fn the_rain_band_inks_nothing_below_ten_percent() {
        assert_eq!(rain_sixteenths(None), None);
        assert_eq!(rain_sixteenths(Some(0)), None);
        assert_eq!(rain_sixteenths(Some(9)), None);
        assert_eq!(rain_sixteenths(Some(10)), Some(2));
        assert_eq!(rain_sixteenths(Some(29)), Some(5));
        assert_eq!(rain_sixteenths(Some(50)), Some(8));
        assert_eq!(rain_sixteenths(Some(69)), Some(11));
        assert_eq!(rain_sixteenths(Some(100)), Some(16));
    }

    #[test]
    fn band_rows_fill_from_the_floor() {
        assert_eq!(band_glyph(16, 0), Some("█"));
        assert_eq!(band_glyph(16, 1), Some("█"));
        assert_eq!(band_glyph(16, 2), None);
        assert_eq!(band_glyph(3, 0), Some("▃"));
        assert_eq!(band_glyph(3, 1), None);
        assert_eq!(band_glyph(11, 1), Some("▃"));
        assert_eq!(band_glyph(0, 0), None);
    }

    #[test]
    fn a_flat_temperature_range_uses_the_middle_step() {
        assert_eq!(temperature_step(Some(12.0), Some((12.0, 12.0))), "▄");
        assert_eq!(temperature_step(None, Some((12.0, 12.0))), " ");
    }

    #[test]
    fn rain_probability_uses_the_week_strip_thresholds() {
        for (chance, expected) in [
            (None, " "),
            (Some(0), "·"),
            (Some(9), "·"),
            (Some(10), "▂"),
            (Some(29), "▂"),
            (Some(30), "▄"),
            (Some(49), "▄"),
            (Some(50), "▆"),
            (Some(69), "▆"),
            (Some(70), "█"),
            (Some(100), "█"),
        ] {
            assert_eq!(rain_step(chance), expected);
        }
    }

    #[test]
    fn wind_uses_eight_sectors_and_a_calm_dot() {
        assert_eq!(wind_symbol(Some(0.5), None), "·");
        assert_eq!(wind_symbol(Some(10.0), Some(0.0)), "↑");
        assert_eq!(wind_symbol(Some(10.0), Some(45.0)), "↗");
        assert_eq!(wind_symbol(Some(10.0), Some(90.0)), "→");
        assert_eq!(wind_symbol(Some(10.0), Some(225.0)), "↙");
        assert_eq!(wind_symbol(Some(10.0), Some(-45.0)), "↖");
        assert_eq!(wind_symbol(Some(10.0), Some(337.5)), "↑");
        assert_eq!(wind_symbol(Some(10.0), Some(360.0)), "↑");
        assert_eq!(wind_symbol(Some(10.0), None), " ");
    }

    #[test]
    fn wind_direction_without_speed_does_not_claim_a_reading() {
        assert_eq!(wind_symbol(None, Some(90.0)), " ");
    }

    #[test]
    fn wind_summary_reports_the_visible_minimum_and_maximum() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now) {
            hour.wind_kph = None;
        }
        weather.hourly[now].wind_kph = Some(5.0);
        weather.hourly[now + 1].wind_kph = Some(15.0);
        weather.hourly[now + 2].wind_kph = Some(10.0);
        let visible = &weather.forecast_hours()[..3];

        assert_eq!(wind_summary(visible, Unit::Metric), "5–15 km/h");
        assert_eq!(wind_summary(visible, Unit::Imperial), "3–9 mph");
    }

    #[test]
    fn wind_summary_keeps_a_flat_range_and_marks_all_missing() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now) {
            hour.wind_kph = Some(10.0);
        }
        assert_eq!(
            wind_summary(&weather.forecast_hours()[..12], Unit::Metric),
            "10–10 km/h"
        );

        for hour in weather.hourly.iter_mut().skip(now) {
            hour.wind_kph = None;
        }
        assert_eq!(
            wind_summary(&weather.forecast_hours()[..12], Unit::Metric),
            "—"
        );
    }

    /// The sky row carries the condition's emoji every third clock hour on
    /// a faint rail. The rail covers known hours between marks, each
    /// two-cell emoji spans its two-cell hour column exactly, and a missing
    /// code leaves a gap rather than pretending the sky held.
    #[test]
    fn the_sky_row_carries_emoji_on_a_three_hour_cadence() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now) {
            hour.code = Some(0);
        }
        for hour in weather.hourly.iter_mut().skip(now + 3).take(3) {
            hour.code = Some(61);
        }
        for hour in weather.hourly.iter_mut().skip(now + 9).take(3) {
            hour.code = None;
        }

        let buffer = rendered_buffer_in(
            &weather,
            80,
            FULL_ROWS,
            0,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );

        // The forecast opens at midnight, so the cadence lands on offsets
        // 0, 3, 6, ... and each emoji starts at its hour's first cell.
        assert_eq!(
            buffer[(12, FULL_SKY_Y)].symbol(),
            "\u{2600}\u{fe0f}",
            "clear at 12a"
        );
        assert_eq!(
            buffer[(18, FULL_SKY_Y)].symbol(),
            "\u{1F327}\u{fe0f}",
            "rain at 3a"
        );
        assert_eq!(
            buffer[(24, FULL_SKY_Y)].symbol(),
            "\u{2600}\u{fe0f}",
            "clear at 6a"
        );
        assert_eq!(
            buffer[(36, FULL_SKY_Y)].symbol(),
            "\u{2600}\u{fe0f}",
            "clear at 12p"
        );
        assert_eq!(
            buffer[(13, FULL_SKY_Y)].symbol(),
            " ",
            "the emoji's second cell must stay clear of the rail"
        );
        for x in [14, 17, 20, 23, 26, 29, 58] {
            assert_eq!(
                buffer[(x, FULL_SKY_Y)].symbol(),
                "\u{2500}",
                "a known off-cadence hour should carry the rail at {x}"
            );
        }
        for x in 30..=35 {
            assert_eq!(
                buffer[(x, FULL_SKY_Y)].symbol(),
                " ",
                "a missing code should leave a gap at {x}"
            );
        }
    }

    /// The cadence follows the clock, not the window edge. A forecast that
    /// opens at 5 PM keeps its first mark for 6 PM, the next real multiple of
    /// three, so the marks stay put as the selection pages through the week.
    #[test]
    fn the_sky_cadence_follows_the_clock_not_the_window_edge() {
        let mut weather = Weather::fixture(22, 14);
        weather.now_hour += 17;
        for hour in weather.hourly.iter_mut() {
            hour.code = Some(0);
        }

        let buffer = rendered_buffer_in(
            &weather,
            80,
            FULL_ROWS,
            0,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );

        assert_eq!(
            buffer[(12, FULL_SKY_Y)].symbol(),
            "\u{2500}",
            "5 PM is off cadence and should carry only the rail"
        );
        for (x, hour) in [(14, "6p"), (20, "9p"), (26, "12a")] {
            assert_eq!(
                buffer[(x, FULL_SKY_Y)].symbol(),
                "\u{2600}\u{fe0f}",
                "expected the mark for {hour} at {x}"
            );
        }
    }

    /// A full layout whose plot affords only one cell per hour cannot hold a
    /// two-cell emoji; the sky row falls back to the one-cell text symbols on
    /// the same cadence.
    #[test]
    fn a_narrow_full_layout_falls_back_to_one_cell_sky_symbols() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now) {
            hour.code = Some(0);
        }

        let width = 40;
        assert_eq!(window_for(width, 0, 192).cell_width, 1);
        let buffer = rendered_buffer_in(
            &weather,
            width,
            FULL_ROWS,
            0,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );

        for x in [10, 13, 16, 19] {
            assert_eq!(
                buffer[(x, FULL_SKY_Y)].symbol(),
                "\u{25cb}",
                "cadence hour at {x} should fall back to the text symbol"
            );
        }
        for x in [11, 12, 14, 15, 17, 18, 20, 21] {
            assert_eq!(
                buffer[(x, FULL_SKY_Y)].symbol(),
                "\u{2500}",
                "off-cadence hour at {x} should carry the rail"
            );
        }
    }

    /// Timestamps that will not parse cannot anchor the cadence to the
    /// clock; the row counts thirds from the window edge instead of hiding
    /// the sky behind a broken clock.
    #[test]
    fn unparseable_timestamps_fall_back_to_a_window_cadence() {
        let mut weather = Weather::fixture(22, 14);
        for hour in weather.hourly.iter_mut() {
            hour.time = "not-a-time".to_string();
            hour.code = Some(0);
        }

        let buffer = rendered_buffer_in(
            &weather,
            80,
            FULL_ROWS,
            0,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );

        for x in [12, 18, 24, 30] {
            assert_eq!(
                buffer[(x, FULL_SKY_Y)].symbol(),
                "\u{2600}\u{fe0f}",
                "every third visible column should carry the mark at {x}"
            );
        }
        for x in [14, 17, 20, 23] {
            assert_eq!(
                buffer[(x, FULL_SKY_Y)].symbol(),
                "\u{2500}",
                "the rail should hold between fallback marks at {x}"
            );
        }
    }

    /// Every visible hour with a reading inks every cell of its column, so the
    /// silhouette reads as one shape instead of a picket fence.
    #[test]
    fn the_silhouette_floor_is_continuous() {
        let weather = Weather::fixture(22, 14);
        let buffer = rendered_buffer_in(
            &weather,
            80,
            FULL_ROWS,
            0,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );
        for x in 12..60 {
            assert!(
                RAMP.contains(&buffer[(x, FULL_TEMP_FLOOR_Y)].symbol()),
                "column {x} broke the silhouette floor"
            );
        }
    }

    /// Wind draws every second hour with speeds on the six-hour ticks, and
    /// keeps the digits away from the selection's own arrow.
    #[test]
    fn the_wind_row_keeps_a_sparse_cadence_with_speeds_on_the_ticks() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now) {
            hour.wind_kph = Some(20.0);
            hour.wind_dir_deg = Some(0.0);
        }
        let palette = Theme::default().palette();

        let buffer = rendered_buffer_in(&weather, 80, FULL_ROWS, 3, false, palette, Unit::Metric);
        let arrows: Vec<u16> = (0..80)
            .filter(|x| buffer[(*x, FULL_WIND_Y)].symbol() == "↑")
            .collect();
        assert_eq!(
            arrows,
            vec![12, 16, 18, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56],
            "even clock hours plus the selected hour"
        );
        assert_eq!(buffer[(13, FULL_WIND_Y)].symbol(), "2", "tick speed digits");
        assert_eq!(buffer[(14, FULL_WIND_Y)].symbol(), "0", "tick speed digits");

        let beside = rendered_buffer_in(&weather, 80, FULL_ROWS, 1, false, palette, Unit::Metric);
        assert_eq!(
            beside[(13, FULL_WIND_Y)].symbol(),
            " ",
            "digits beside the selection would collide with its arrow"
        );
    }

    /// The selection keeps its shape in monochrome, while the opening label
    /// still states the current day and time without a separate marker.
    #[test]
    fn selected_marker_and_current_time_anchor_survive_a_monochrome_palette() {
        let weather = Weather::fixture(22, 14);
        let monochrome = Palette {
            accent: Color::Gray,
            text: Color::Gray,
            muted: Color::Gray,
            selection: Color::Gray,
            now: Color::Gray,
            error: Color::Gray,
            border: Color::Gray,
        };

        for palette in [Theme::default().palette(), monochrome] {
            let text = rendered_in(&weather, 80, FULL_ROWS, 3, false, palette, Unit::Metric);
            assert!(text.contains('▲'), "selection lost without colour:\n{text}");
            assert!(
                text.contains("Sun 12a"),
                "current day and time lost without colour:\n{text}"
            );
            assert!(
                !text.contains('┬'),
                "redundant current marker returned:\n{text}"
            );
        }
    }

    /// The current time label itself is the indicator. Giving it the `now`
    /// role keeps the day and time readable without stacking a separate glyph
    /// above the first weather column.
    #[test]
    fn current_time_anchor_replaces_the_separate_marker() {
        let weather = Weather::fixture(22, 14);
        let palette = Theme::default().palette();

        for (width, rows, compact) in [(34, COMPACT_ROWS, true), (80, FULL_ROWS, false)] {
            let buffer =
                rendered_buffer_in(&weather, width, rows, 3, compact, palette, Unit::Metric);
            let text: String = (0..rows)
                .map(|y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
                .join("\n");
            let axis_y = if compact { 0 } else { FULL_AXIS_Y };
            let anchor_x = (1..width - 1)
                .find(|x| buffer[(*x, axis_y)].symbol() == "S")
                .expect("current time anchor");
            let axis: String = (0..width).map(|x| buffer[(x, axis_y)].symbol()).collect();

            assert!(
                axis.contains("Sun") && axis.contains("12a"),
                "current day and time disappeared from {axis:?}"
            );
            assert_eq!(buffer[(anchor_x, axis_y)].fg, palette.now);
            assert!(
                !text.contains('┬'),
                "redundant current marker returned:\n{text}"
            );
            assert!(text.contains('▲'), "selection marker missing:\n{text}");

            if !compact {
                let label_x = (1..width - 3)
                    .find(|x| {
                        buffer[(*x, FULL_SKY_Y)].symbol() == "s"
                            && buffer[(*x + 1, FULL_SKY_Y)].symbol() == "k"
                            && buffer[(*x + 2, FULL_SKY_Y)].symbol() == "y"
                    })
                    .expect("sky label");
                assert_eq!(anchor_x, label_x + LABEL_WIDTH);
            }
        }
    }

    /// Once navigation has moved to a later page there is no current-hour
    /// marker in the first plot cell. The axis must stop reserving a phantom
    /// column for it and put the leading anchor on the first hour.
    #[test]
    fn a_scrolled_page_does_not_keep_the_current_markers_offset() {
        let weather = Weather::fixture(22, 14);
        let width = 80;
        let buffer = rendered_buffer_in(
            &weather,
            width,
            FULL_ROWS,
            24,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );
        let label_x = (1..width - 3)
            .find(|x| {
                buffer[(*x, FULL_SKY_Y)].symbol() == "s"
                    && buffer[(*x + 1, FULL_SKY_Y)].symbol() == "k"
                    && buffer[(*x + 2, FULL_SKY_Y)].symbol() == "y"
            })
            .expect("sky label");
        let plot_x = label_x + LABEL_WIDTH;
        let first_axis_x = (1..width - 1)
            .find(|x| !buffer[(*x, FULL_AXIS_Y)].symbol().trim().is_empty())
            .expect("leading axis anchor");

        assert_eq!(first_axis_x, plot_x);
        assert_eq!(
            buffer[(first_axis_x, FULL_AXIS_Y)].fg,
            Theme::default().palette().muted,
            "a later page presented its anchor as the current time"
        );
        assert!(
            marker_coordinates(&buffer, width, FULL_ROWS, "┬").is_empty(),
            "current marker leaked onto a scrolled page"
        );
    }

    /// The plot, labels, and summaries are one measured unit. Centering that
    /// unit prevents the 24-hour tier from leaving a conspicuous void only on
    /// its right at the 87-column Apple Terminal layout.
    #[test]
    fn the_24_hour_weathergram_is_balanced_inside_its_frame() {
        let weather = Weather::fixture(22, 14);
        let width = 87;
        let buffer = rendered_buffer_in(
            &weather,
            width,
            FULL_ROWS,
            0,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );
        let occupied: Vec<u16> = (1..width - 1)
            .filter(|x| !buffer[(*x, FULL_TEMP_LABEL_Y)].symbol().trim().is_empty())
            .collect();
        let first = *occupied.first().expect("temperature row content");
        let last = *occupied.last().expect("temperature row content");
        let left_slack = first - 1;
        let right_slack = (width - 2) - last;

        assert!(
            left_slack.abs_diff(right_slack) <= 1,
            "weathergram is not centred: left {left_slack}, right {right_slack}"
        );
    }

    #[test]
    fn every_theme_keeps_the_selection_and_current_anchor_in_place() {
        let weather = Weather::fixture(22, 14);
        let baseline = rendered_buffer_in(
            &weather,
            80,
            FULL_ROWS,
            3,
            false,
            Theme::default().palette(),
            Unit::Metric,
        );
        let selected = marker_coordinates(&baseline, 80, FULL_ROWS, "▲");
        let anchor_x = (1..79)
            .find(|x| baseline[(*x, FULL_AXIS_Y)].symbol() == "S")
            .expect("current time anchor");

        for theme in Theme::ALL {
            let palette = theme.palette();
            let buffer =
                rendered_buffer_in(&weather, 80, FULL_ROWS, 3, false, palette, Unit::Metric);
            assert_eq!(
                marker_coordinates(&buffer, 80, FULL_ROWS, "▲"),
                selected,
                "{} moved the selected marker",
                theme.name()
            );
            assert_eq!(
                buffer[(anchor_x, FULL_AXIS_Y)].fg,
                palette.now,
                "{} did not style the current time anchor",
                theme.name()
            );
        }
    }

    #[test]
    fn full_weathergram_draws_four_aligned_tracks_and_markers() {
        let weather = Weather::fixture(22, 14);
        let text = rendered(&weather, 80, FULL_ROWS, 3, false);

        for label in ["sky", "temp", "rain", "wind"] {
            assert!(text.contains(label), "missing {label}:\n{text}");
        }
        assert!(text.contains('▲'), "selection has no shape marker:\n{text}");
        assert!(text.contains("next 24 h"), "wrong horizon:\n{text}");
    }

    #[test]
    fn compact_weathergram_keeps_every_track_in_six_rows() {
        let weather = Weather::fixture(22, 14);
        let text = rendered(&weather, 34, COMPACT_ROWS, 0, true);

        assert_eq!(text.lines().count(), COMPACT_ROWS as usize);
        for label in ["sky", "temp", "rain", "wind"] {
            assert!(text.contains(label), "missing {label}:\n{text}");
        }
    }

    #[test]
    fn every_width_uses_its_quantized_horizon() {
        let weather = Weather::fixture(22, 14);

        for (width, hours, compact) in [
            (34, 12, true),
            (80, 24, false),
            (100, 36, false),
            (120, 48, false),
        ] {
            let text = rendered(
                &weather,
                width,
                if compact { COMPACT_ROWS } else { FULL_ROWS },
                0,
                compact,
            );
            assert_eq!(
                window_for(width, 0, weather.forecast_hours().len()).hours,
                hours
            );
            if compact {
                assert!(
                    text.contains("Hourly"),
                    "compact horizon at {width}:\n{text}"
                );
                let wind = text
                    .lines()
                    .nth(4)
                    .unwrap_or_else(|| panic!("compact wind row missing at {width}:\n{text}"));
                assert_eq!(wind.chars().nth(18), Some('↘'), "twelfth hour:\n{text}");
                assert_eq!(wind.chars().nth(19), Some(' '), "thirteenth hour:\n{text}");
            } else {
                assert!(
                    text.contains(&format!("next {hours} h")),
                    "horizon at {width}:\n{text}"
                );
            }
        }
    }

    #[test]
    fn empty_and_missing_weathergrams_draw_without_panicking() {
        let mut empty = Weather::fixture(22, 14);
        empty.hourly.clear();
        let _ = rendered(&empty, 80, FULL_ROWS, 0, false);

        let mut missing = Weather::fixture(22, 14);
        for hour in &mut missing.hourly {
            hour.code = None;
            hour.temp_c = None;
            hour.chance = None;
            hour.wind_kph = None;
            hour.wind_dir_deg = None;
            hour.precip_mm = None;
        }
        let _ = rendered(&missing, 80, FULL_ROWS, 0, false);
    }

    #[test]
    fn all_missing_precipitation_is_unavailable_in_the_summary_and_renderer() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now) {
            hour.precip_mm = None;
        }

        assert_eq!(
            precipitation_summary(weather.forecast_hours(), 0, Unit::Metric),
            "—"
        );
        let text = rendered(&weather, 80, FULL_ROWS, 0, false);
        let rain = text
            .lines()
            .find(|line| line.contains("rain"))
            .unwrap_or_else(|| panic!("rain row missing:\n{text}"));
        assert!(
            rain.contains('—'),
            "missing precipitation reads as dry:\n{text}"
        );
    }

    #[test]
    fn partial_precipitation_is_unavailable_in_the_summary() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now).take(24) {
            hour.precip_mm = Some(0.0);
        }
        weather.hourly[now + 7].precip_mm = None;

        assert_eq!(
            precipitation_summary(weather.forecast_hours(), 0, Unit::Metric),
            "—"
        );
    }

    #[test]
    fn precipitation_summary_distinguishes_zero_and_trace() {
        let mut weather = Weather::fixture(22, 14);
        let now = weather.now_hour;
        for hour in weather.hourly.iter_mut().skip(now).take(24) {
            hour.precip_mm = Some(0.0);
        }

        assert_eq!(
            precipitation_summary(weather.forecast_hours(), 0, Unit::Metric),
            "0 mm"
        );
        assert_eq!(
            precipitation_summary(weather.forecast_hours(), 0, Unit::Imperial),
            "0 in"
        );

        weather.hourly[now].precip_mm = Some(0.01);
        assert_eq!(
            precipitation_summary(weather.forecast_hours(), 0, Unit::Metric),
            "<0.1 mm"
        );
        assert_eq!(
            precipitation_summary(weather.forecast_hours(), 0, Unit::Imperial),
            "<0.01 in"
        );
    }

    fn rendered(
        weather: &Weather,
        width: u16,
        height: u16,
        selected: usize,
        compact: bool,
    ) -> String {
        rendered_in(
            weather,
            width,
            height,
            selected,
            compact,
            Theme::default().palette(),
            Unit::Metric,
        )
    }

    fn rendered_in(
        weather: &Weather,
        width: u16,
        height: u16,
        selected: usize,
        compact: bool,
        palette: Palette,
        unit: Unit,
    ) -> String {
        let buffer = rendered_buffer_in(weather, width, height, selected, compact, palette, unit);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn rendered_buffer_in(
        weather: &Weather,
        width: u16,
        height: u16,
        selected: usize,
        compact: bool,
        palette: Palette,
        unit: Unit,
    ) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                weathergram_render(
                    frame,
                    weather.forecast_hours(),
                    palette,
                    frame.area(),
                    unit,
                    selected,
                    compact,
                )
            })
            .unwrap();

        terminal.backend().buffer().clone()
    }

    fn marker_coordinates(
        buffer: &Buffer,
        width: u16,
        height: u16,
        marker: &str,
    ) -> Vec<(u16, u16)> {
        (0..height)
            .flat_map(|y| {
                (0..width)
                    .filter(move |x| buffer[(*x, y)].symbol() == marker)
                    .map(move |x| (x, y))
            })
            .collect()
    }
}
