use crate::theme::Palette;
use crate::ui::axis::{hour_ticks_render, put, put_right, put_text};
use crate::ui::bars::window_start;
use crate::ui::condition_symbol;
use crate::units::Unit;
use crate::weather::model::HourlyForecast;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::Block;

pub(super) const FULL_ROWS: u16 = 8;
pub(super) const COMPACT_ROWS: u16 = 6;

const BORDER_COLS: u16 = 2;
const LABEL_WIDTH: u16 = 6;
const SUMMARY_GAP: u16 = 1;
const SUMMARY_WIDTH: u16 = 12;
const HORIZONS: [usize; 3] = [48, 36, 24];
const COMPACT_HORIZON: usize = 12;
const MAX_CELL_WIDTH: u16 = 3;

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
    if speed_kph.is_some_and(|speed| speed < 1.0) {
        return "·";
    }
    let Some(degrees) = direction else {
        return " ";
    };
    let normalized = degrees.rem_euclid(360.0);
    ARROWS[((normalized / 45.0).round() as usize) % ARROWS.len()]
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
    let Some(total) = window
        .iter()
        .filter_map(|hour| hour.precip_mm)
        .reduce(|total, value| total + value)
    else {
        return "—".to_string();
    };
    let decimals = unit.precip_decimals();
    format!("{:.decimals$} {}", unit.precip(total), unit.precip_label())
}

fn wind_summary(hours: &[HourlyForecast], unit: Unit) -> String {
    hours
        .iter()
        .filter_map(|hour| hour.wind_kph)
        .reduce(f64::max)
        .map_or_else(
            || "—".to_string(),
            |speed| format!("{:.0} {}", unit.speed(speed), unit.speed_label()),
        )
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
            Line::from(format!("Hourly weather · next {} h", window.hours)).fg(palette.muted),
        )
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let plot_x = inner.x + LABEL_WIDTH;
    let plot_width = window.hours as u16 * window.cell_width;
    let summary_x = plot_x + plot_width + SUMMARY_GAP;
    let axis_y = if compact { area.y } else { inner.y };
    let first_track_y = if compact { inner.y } else { inner.y + 1 };
    let marker_y = if compact {
        area.bottom().saturating_sub(1)
    } else {
        inner.bottom().saturating_sub(1)
    };

    if compact {
        put_text(frame, area.x + 1, area.y, "Hourly", palette.muted);
    }
    hour_ticks_render(
        frame,
        Rect::new(plot_x, axis_y, plot_width, 1),
        visible.iter().map(|hour| hour.time.as_str()),
        window.cell_width,
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
        put_text(frame, inner.x, y, label, palette.muted);
        put_right(frame, summary_x, y, SUMMARY_WIDTH, &summary, palette.muted);

        for (offset, hour) in visible.iter().enumerate() {
            let index = window.start + offset;
            let symbol = match row {
                0 => condition_symbol::symbol(hour.code),
                1 => temperature_step(hour.temp_c, range),
                2 => rain_step(hour.chance),
                _ => wind_symbol(hour.wind_kph, hour.wind_dir_deg),
            };
            let colour = if index == selected {
                palette.selection
            } else if index == 0 {
                palette.now
            } else if matches!(row, 1 | 2) {
                palette.accent
            } else {
                palette.text
            };
            let x = plot_x + offset as u16 * window.cell_width + (window.cell_width - 1) / 2;
            put(frame, x, y, symbol, colour);
        }
    }

    if window.start == 0 {
        put(frame, plot_x, axis_y, "┬", palette.now);
    }
    if selected >= window.start && selected < end {
        let offset = selected - window.start;
        let x = plot_x + offset as u16 * window.cell_width + (window.cell_width - 1) / 2;
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

    /// The selected hour and the current hour communicate with shapes as well
    /// as colours, so neither state disappears for a monochrome terminal.
    #[test]
    fn selected_and_current_hour_markers_survive_a_monochrome_palette() {
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
                text.contains('┬'),
                "current-hour axis mark lost without colour:\n{text}"
            );
        }
    }

    #[test]
    fn every_theme_keeps_the_selected_and_current_markers_in_place() {
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
        let current = marker_coordinates(&baseline, 80, FULL_ROWS, "┬");

        for theme in Theme::ALL {
            let buffer = rendered_buffer_in(
                &weather,
                80,
                FULL_ROWS,
                3,
                false,
                theme.palette(),
                Unit::Metric,
            );
            assert_eq!(
                marker_coordinates(&buffer, 80, FULL_ROWS, "▲"),
                selected,
                "{} moved the selected marker",
                theme.name()
            );
            assert_eq!(
                marker_coordinates(&buffer, 80, FULL_ROWS, "┬"),
                current,
                "{} moved the current-hour marker",
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
