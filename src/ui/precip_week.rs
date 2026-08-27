//! The week strip: every forecast day, one row each, on a shared hour axis.
//!
//! This is the view the hourly chart cannot be. That chart spends its width on
//! bars broad enough to read, which buys about thirty hours of an eight-day
//! series; the rest of the week is off the right-hand edge, reachable only by
//! arrowing to it. Here every day is a row and every hour a cell, so the whole
//! series is on screen at once and the question it answers — *which day should
//! I move this outdoors to* — takes a glance rather than a scroll.
//!
//! The rows share one axis, and that is the whole point of the form. Column 30
//! is 3 PM in every row, so a wet afternoon on Tuesday sits directly above a
//! wet afternoon on Wednesday and the pattern is visible down the grid. An
//! earlier attempt wrapped the bar chart into stacked bands instead, which
//! looks like this and is not: those bands began wherever the previous one ran
//! out — Sat 12a, Sun 5a, Mon 10a — so no two rows were comparable and the
//! stack read as one long sentence with arbitrary line breaks.
//!
//! Chance only. Amount is a different quantity on a different scale, and
//! shading a cell by one while labelling the row with the other is how a chart
//! invents a correlation. It rides the right-hand gutter as a per-day total,
//! which is a table column and reads as one.

use crate::theme::Palette;
use crate::ui::axis::{clock, put_right, put_styled, put_text};
use crate::units::Unit;
use crate::weather::model::HourlyForecast;
use chrono::{NaiveDateTime, Timelike};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::Block;

const HOURS_IN_A_DAY: usize = 24;

/// The ramp: five steps, banded where a forecast changes what you would do. A
/// 30% afternoon and a 70% one are different plans; 64% and 68% are the same
/// plan, so the scale is deliberately coarse.
///
/// Height, not density. The first draft shaded with `░▒▓█`, which varies only
/// how finely a cell is dithered — at one row tall those read as four barely
/// distinguishable greys, and telling a likely hour from an unlikely one took a
/// second look rather than a glance. These vary how much ink a cell holds *and*
/// where it sits in the row, so a day has a silhouette. It is also the
/// vocabulary the hourly chart above already draws its bars from, which is one
/// less thing for the screen to teach.
///
/// `faint` dims the two lowest steps: a second channel on the same ramp that
/// costs no columns and works in every theme, because `DIM` acts on whatever
/// foreground the palette handed over rather than on a shade each theme would
/// have to define. No step is identified by the dimming alone, so a terminal
/// that ignores SGR 2 loses contrast and nothing else. It applies to the ramp
/// only: an hour carrying the selection or now is drawn at full strength
/// whatever step it falls in, because those are states rather than readings.
const SHADE: [Step; 5] = [
    Step::faint(10, "·"),
    Step::faint(30, "▂"),
    Step::solid(50, "▄"),
    Step::solid(70, "▆"),
    Step::solid(u8::MAX, "█"),
];

struct Step {
    /// The reading this step covers everything below.
    upper: u8,
    symbol: &'static str,
    faint: bool,
}

impl Step {
    const fn faint(upper: u8, symbol: &'static str) -> Self {
        Self {
            upper,
            symbol,
            faint: true,
        }
    }

    const fn solid(upper: u8, symbol: &'static str) -> Self {
        Self {
            upper,
            symbol,
            faint: false,
        }
    }
}

/// No reading at all — drawn as nothing, rather than as the bottom step, which
/// would claim the API said "dry" when it said nothing.
const NOTHING: Step = Step::solid(0, MISSING);

/// Hours with no reading at all. Distinct from a dry hour, which draws the
/// faintest step of the ramp rather than nothing.
const MISSING: &str = " ";

/// Columns the day name takes: a marker, the longest name, and a space. Sized
/// from `TODAY` rather than from a weekday abbreviation, which is what the
/// three-letter slot used to hold.
const DAY_WIDTH: u16 = 1 + TODAY.len() as u16 + 1;

/// The first row of the strip is always the current day — the series it is
/// built from starts at the current hour. Naming it rather than dating it
/// matches the pane above and the weather screen's own day inspector, and it
/// saves the reader working out which weekday today is.
const TODAY: &str = "Today";
/// Columns the per-day total takes, e.g. "0.08 in".
const TOTAL_WIDTH: u16 = 8;
/// The row of hour labels above the grid.
const AXIS_ROWS: u16 = 1;
/// Past three columns an hour the grid reads as blocks rather than a pattern.
const MAX_CELL: u16 = 3;

/// Marks the day the selection is on, so the strip says which row is live
/// without relying on the cell colour alone.
const SELECTED_DAY: &str = "▸";

/// One clause, so it needs no interpunct to join two halves.
const TITLE: &str = "Chance of precipitation this week";

/// Rows the box wants for `days`: the grid, its axis, and two borders.
pub(super) fn box_rows(days: usize) -> u16 {
    days as u16 + AXIS_ROWS + 2
}

/// Fewer rows than this and the strip is a caption with a grid attached; the
/// hourly chart is the better use of them.
pub(super) const MIN_DAYS: usize = 3;

/// Narrower than this the grid cannot give an hour a column and still name the
/// day beside it.
pub(super) const MIN_WIDTH: u16 = DAY_WIDTH + HOURS_IN_A_DAY as u16 + 2;

pub(super) fn precip_week_render(
    frame: &mut Frame,
    days: &[Day],
    palette: Palette,
    area: Rect,
    unit: Unit,
    selected: usize,
) {
    let block = Block::bordered()
        .border_style(Style::new().fg(palette.border))
        // Right, opposite the chart's title above it, and phrased as one
        // clause so it needs no separator to hold two halves apart.
        .title_top(Line::from(TITLE).fg(palette.muted).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width < MIN_WIDTH || inner.height <= AXIS_ROWS {
        return;
    }

    let shown = days.len().min((inner.height - AXIS_ROWS) as usize);

    // Wider cells where there is room. One column an hour is legible but
    // cramped — a single wet hour reads as a speck — and past three the grid
    // is blocks rather than a pattern.
    let totals = inner.width >= MIN_WIDTH + TOTAL_WIDTH + HOURS_IN_A_DAY as u16;
    let gutters = DAY_WIDTH + if totals { TOTAL_WIDTH + 1 } else { 0 };
    let cell = ((inner.width - gutters) / HOURS_IN_A_DAY as u16).clamp(1, MAX_CELL);

    // Centred on what it measures, like both charts. Pinned left, a 48-column
    // grid in a 118-column box reads as a chart that failed to load the rest.
    let grid_width = HOURS_IN_A_DAY as u16 * cell;
    let used = gutters + grid_width;
    let left = inner.x + (inner.width - used) / 2;
    let grid_x = left + DAY_WIDTH;

    hour_axis_render(frame, inner.y, grid_x, cell, palette);

    for (row, day) in days.iter().take(shown).enumerate() {
        let y = inner.y + AXIS_ROWS + row as u16;
        let holds_selection = day.hours.iter().flatten().any(|(i, _)| *i == selected);

        if holds_selection {
            put_text(frame, left, y, SELECTED_DAY, palette.selection);
        }
        let name = if holds_selection {
            palette.selection
        } else {
            palette.muted
        };
        put_text(frame, left + 1, y, &day.name, name);

        for (hour, slot) in day.hours.iter().enumerate() {
            let x = grid_x + hour as u16 * cell;
            let Some((index, forecast)) = slot else {
                continue;
            };

            // Shade carries the reading; colour carries which hour it is. Two
            // channels, so neither has to do the other's work — and the hour
            // the arrows are on is stated exactly by the hero above, which is
            // what keeps a colour-only cue honest here.
            let colour = if *index == selected {
                palette.selection
            } else if *index == 0 {
                palette.now
            } else {
                palette.accent
            };

            // State outranks the ramp. The low steps are dimmed to push the
            // quiet end of the scale back, but the hour the arrows are on is
            // not part of that scale — and a dimmed selection colour on a dark
            // ground is very nearly invisible, which is what it looked like for
            // every selected hour under 30%. Marked hours are drawn at full
            // strength and bold instead.
            let step = shade(forecast.chance);
            let style = match (*index == selected || *index == 0, step.faint) {
                (true, _) => Style::new().fg(colour).add_modifier(Modifier::BOLD),
                (false, true) => Style::new().fg(colour).add_modifier(Modifier::DIM),
                (false, false) => Style::new().fg(colour),
            };

            for offset in 0..cell {
                put_styled(frame, x + offset, y, step.symbol, style);
            }
        }

        if totals {
            let total = day_total(day, unit);
            put_right(
                frame,
                grid_x + grid_width + 1,
                y,
                TOTAL_WIDTH,
                &total,
                palette.text,
            );
        }
    }
}

/// Hour labels over the grid. Every sixth hour, which is four marks across a
/// day — enough to place an afternoon without turning the row into a ruler.
fn hour_axis_render(frame: &mut Frame, y: u16, grid_x: u16, cell: u16, palette: Palette) {
    for hour in (0..HOURS_IN_A_DAY as u32).step_by(6) {
        put_text(
            frame,
            grid_x + hour as u16 * cell,
            y,
            &clock(hour),
            palette.muted,
        );
    }
}

/// One forecast day: its name, and its hours in the slots they actually fall
/// in. The first day is nearly always partial — the series starts at the
/// current hour — and its leading slots stay empty so the columns still line
/// up with every row beneath it.
///
/// The hours are borrowed rather than cloned. The grouping runs on every frame
/// of the precipitation screen, and cloning put nearly two hundred heap
/// allocations into each one for series the caller already holds.
pub(super) struct Day<'a> {
    name: String,
    hours: [Option<(usize, &'a HourlyForecast)>; HOURS_IN_A_DAY],
}

/// The strip's rows, one per calendar date the series touches.
///
/// The one place a date is derived from an hour, and the caller reserves the
/// rows this returns. The strip draws a row per calendar date; while the
/// layout made its own calculation — dividing the hour count by 24 — the two
/// disagreed for every window that did not open at midnight, and the strip
/// silently lost its last row.
pub(super) fn group_by_day(hours: &[HourlyForecast]) -> Vec<Day<'_>> {
    let mut days = grouped(hours);

    // Hour zero of the forward window is now, so whichever day it landed in is
    // today by construction — there is no date arithmetic to get wrong here.
    if let Some(today) = days.first_mut() {
        today.name = TODAY.to_string();
    }
    days
}

fn grouped(hours: &[HourlyForecast]) -> Vec<Day<'_>> {
    let mut days: Vec<Day> = Vec::new();
    let mut current = None;

    for (index, hour) in hours.iter().enumerate() {
        let Some(at) = date_of(hour) else { continue };

        if current != Some(at.date()) {
            current = Some(at.date());
            days.push(Day {
                name: at.format("%a").to_string(),
                hours: [const { None }; HOURS_IN_A_DAY],
            });
        }

        if let Some(day) = days.last_mut() {
            day.hours[at.hour() as usize % HOURS_IN_A_DAY] = Some((index, hour));
        }
    }

    days
}

/// The stamp an hour carries, or `None` where it will not parse. An
/// unparseable stamp must not open a row of its own, or a malformed response
/// would push the strip past the height reserved for it.
fn date_of(hour: &HourlyForecast) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(&hour.time, "%Y-%m-%dT%H:%M").ok()
}

fn shade(chance: Option<u8>) -> &'static Step {
    let Some(chance) = chance else {
        return &NOTHING;
    };
    SHADE
        .iter()
        .find(|step| chance < step.upper)
        .unwrap_or(&SHADE[SHADE.len() - 1])
}

/// The day's forecast accumulation. A dry day says so with a dash rather than
/// with `0.00 in`, which reads as a measurement rather than as an absence.
fn day_total(day: &Day, unit: Unit) -> String {
    let total: f64 = day
        .hours
        .iter()
        .flatten()
        .filter_map(|(_, hour)| hour.precip_mm)
        .sum();

    if total <= 0.0 {
        return "—".to_string();
    }

    let value = unit.precip(total);
    let decimals = unit.precip_decimals();
    let quantum = 0.1_f64.powi(decimals as i32);

    if value < quantum / 2.0 {
        return format!("<{quantum:.decimals$} {}", unit.precip_label());
    }
    format!("{value:.decimals$} {}", unit.precip_label())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use crate::weather::model::Weather;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn palette() -> Palette {
        Theme::default().palette()
    }

    fn hours(count: usize, start_hour: usize) -> Vec<HourlyForecast> {
        (0..count)
            .map(|i| {
                let hour = start_hour + i;
                HourlyForecast {
                    time: format!("2026-08-{:02}T{:02}:00", 10 + hour / 24, hour % 24),
                    precip_mm: Some(0.0),
                    snow_cm: Some(0.0),
                    chance: Some(10),
                    code: Some(0),
                    temp_c: Some(20.0),
                    feels_like_c: Some(19.0),
                    humidity_pct: Some(55),
                    wind_kph: Some(10.0),
                    gust_kph: Some(18.0),
                    wind_dir_deg: Some(225.0),
                }
            })
            .collect()
    }

    /// The guarantee the whole form rests on: an hour lands in the column its
    /// clock time names, in every row. A partial first day must shift its cells
    /// right rather than start the row at the left edge — that was exactly the
    /// bug that made the stacked-band version unreadable.
    #[test]
    fn an_hour_lands_in_the_column_its_clock_names() {
        // Starting at 6 PM: six hours today, two whole days, then a part day.
        let series = hours(60, 18);
        let days = group_by_day(&series);

        assert_eq!(days.len(), 4);
        assert!(
            days[0].hours[..18].iter().all(Option::is_none),
            "the partial first day claimed hours before it started"
        );
        assert!(days[0].hours[18].is_some(), "6 PM is in slot 18");
        assert!(
            days[1].hours[0].is_some(),
            "the next day starts at midnight"
        );
        assert!(days[1].hours.iter().all(Option::is_some), "a whole day");
    }

    /// The bug the grouping exists to prevent, stated on its own so a
    /// regression names it rather than a coordinate: `hours / 24` is the
    /// number of *days* a window spans and not the number of dates it touches,
    /// and the two agree only for a window that opens at midnight.
    #[test]
    fn an_evening_window_is_counted_by_date_and_not_by_elapsed_days() {
        let series = hours(60, 18);

        assert_eq!(
            group_by_day(&series).len(),
            4,
            "6 PM plus 60 hours touches 4 dates"
        );
        assert_eq!(60_usize.div_ceil(24), 3, "where the day count says 3");
        assert!(group_by_day(&[]).is_empty(), "no hours, no rows");
    }

    /// An unparseable stamp must not open a row of its own, or a malformed
    /// response would push the strip past the height reserved for it.
    #[test]
    fn hours_that_will_not_parse_are_not_days() {
        let mut series = hours(48, 0);
        series[5].time = "not-a-time".to_string();

        assert_eq!(group_by_day(&series).len(), 2);
    }

    /// The index carried alongside each hour is what the selection is matched
    /// against, so it has to survive the regrouping.
    #[test]
    fn the_series_index_survives_being_grouped() {
        let series = hours(60, 18);
        let days = group_by_day(&series);

        assert_eq!(days[0].hours[18].as_ref().map(|(i, _)| *i), Some(0));
        assert_eq!(days[0].hours[23].as_ref().map(|(i, _)| *i), Some(5));
        assert_eq!(days[1].hours[0].as_ref().map(|(i, _)| *i), Some(6));
    }

    /// Five steps, and the boundaries are the ones a reader plans against.
    #[test]
    fn the_ramp_steps_where_a_forecast_changes_meaning() {
        assert_eq!(shade(Some(0)).symbol, "·");
        assert_eq!(shade(Some(9)).symbol, "·");
        assert_eq!(shade(Some(10)).symbol, "▂");
        assert_eq!(shade(Some(45)).symbol, "▄");
        assert_eq!(shade(Some(69)).symbol, "▆");
        assert_eq!(shade(Some(70)).symbol, "█");
        assert_eq!(shade(Some(100)).symbol, "█");

        // Rising, so the grid has a silhouette rather than a texture.
        let heights: Vec<&str> = SHADE.iter().map(|s| s.symbol).collect();
        assert_eq!(heights, ["·", "▂", "▄", "▆", "█"]);
        // And the dimming only ever reinforces the low end.
        assert!(SHADE[0].faint && SHADE[1].faint);
        assert!(SHADE[2..].iter().all(|s| !s.faint));
    }

    /// A dry hour and an unreported one are different claims.
    #[test]
    fn a_missing_reading_draws_nothing_rather_than_a_dry_hour() {
        assert_eq!(shade(None).symbol, MISSING);
        assert_ne!(shade(Some(0)).symbol, MISSING);
    }

    #[test]
    fn a_dry_day_reads_as_an_absence_rather_than_a_measurement() {
        let series = hours(48, 0);
        let days = group_by_day(&series);
        assert_eq!(day_total(&days[0], Unit::Imperial), "—");
    }

    #[test]
    fn a_days_total_adds_its_hours_up() {
        let mut series = hours(48, 0);
        for hour in series.iter_mut().take(4) {
            hour.precip_mm = Some(6.35);
        }
        let days = group_by_day(&series);

        assert_eq!(day_total(&days[0], Unit::Imperial), "1.00 in");
        assert_eq!(day_total(&days[1], Unit::Imperial), "—");
    }

    fn rendered(width: u16, height: u16, selected: usize) -> String {
        let weather = Weather::fixture(22, 14);
        let days = group_by_day(weather.forecast_hours());
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|f| {
                precip_week_render(f, &days, palette(), f.area(), Unit::Imperial, selected);
            })
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

    /// The first row is the day you are standing in, and naming it costs the
    /// reader nothing to decode. Every row below it keeps its weekday.
    #[test]
    fn the_current_day_is_named_rather_than_dated() {
        let series = hours(60, 18);
        let days = group_by_day(&series);

        assert_eq!(days[0].name, TODAY);
        assert!(
            days[1..].iter().all(|day| day.name != TODAY),
            "only one day is today: {:?}",
            days.iter().map(|d| &d.name).collect::<Vec<_>>()
        );
        assert_eq!(days[1].name, "Tue", "11 Aug 2026 is a Tuesday");
    }

    /// "Today" is longer than the abbreviation the gutter used to hold, so the
    /// gutter has to be measured from it or the grid starts a column late on
    /// the one row that matters most.
    #[test]
    fn the_day_gutter_fits_the_longest_name_it_draws() {
        assert!(DAY_WIDTH as usize >= TODAY.len() + 2, "{DAY_WIDTH}");
        assert!(
            group_by_day(&hours(60, 18))
                .iter()
                .all(|day| day.name.len() + 2 <= DAY_WIDTH as usize)
        );
    }

    #[test]
    fn the_grid_names_its_days_and_its_hours() {
        let text = rendered(90, 11, 3);

        for label in ["12a", "6a", "12p", "6p"] {
            assert!(text.contains(label), "no {label} on the axis:\n{text}");
        }
        assert!(text.contains("Sat"), "no day names:\n{text}");
    }

    /// The strip is a navigator, so it has to show which row the arrows are on
    /// without depending on the cell colour to say it.
    #[test]
    fn the_selected_day_is_marked_by_shape_as_well_as_colour() {
        let text = rendered(90, 11, 30);
        assert!(
            text.contains(SELECTED_DAY),
            "the selected day is unmarked:\n{text}"
        );
    }

    /// The dimming is a step on the value ramp, and the selection is not a
    /// value. Applied to it, a quiet hour's cell came out dim yellow on a dark
    /// ground — present in the buffer and very nearly invisible on screen, for
    /// every selected hour under 30%.
    #[test]
    fn a_marked_hour_is_never_dimmed_however_quiet_it_is() {
        let mut weather = Weather::fixture(22, 14);
        for hour in &mut weather.hourly {
            // The bottom of the ramp, which is the step that dims.
            hour.chance = Some(5);
        }
        let days = group_by_day(weather.forecast_hours());

        let mut terminal = Terminal::new(TestBackend::new(90, 11)).unwrap();
        terminal
            .draw(|f| {
                precip_week_render(f, &days, palette(), f.area(), Unit::Imperial, 3);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        let marked: Vec<_> = (0..90u16)
            .flat_map(|x| (0..11u16).map(move |y| (x, y)))
            .filter(|&cell| {
                let style = buffer[cell].style();
                style.fg == Some(palette().selection) && buffer[cell].symbol() != SELECTED_DAY
            })
            .collect();

        assert!(
            !marked.is_empty(),
            "nothing was marked, so this proved nothing"
        );
        for cell in marked {
            assert!(
                !buffer[cell].style().add_modifier.contains(Modifier::DIM),
                "the selected cell at {cell:?} is dimmed"
            );
        }
    }

    /// The other half of the pair: the ramp's own low steps must still dim, or
    /// the fix above has quietly removed the second channel altogether.
    #[test]
    fn the_quiet_end_of_the_ramp_still_recedes() {
        let mut weather = Weather::fixture(22, 14);
        for hour in &mut weather.hourly {
            hour.chance = Some(5);
        }
        let days = group_by_day(weather.forecast_hours());

        let mut terminal = Terminal::new(TestBackend::new(90, 11)).unwrap();
        terminal
            .draw(|f| {
                precip_week_render(f, &days, palette(), f.area(), Unit::Imperial, 3);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();

        let dimmed = (0..90u16)
            .flat_map(|x| (0..11u16).map(move |y| (x, y)))
            .filter(|&cell| buffer[cell].style().add_modifier.contains(Modifier::DIM))
            .count();

        assert!(dimmed > 0, "a week of 5% hours should be a quiet grid");
    }

    #[test]
    fn renders_without_panicking_at_awkward_sizes() {
        for (width, height) in [
            (34, 12),
            (40, 4),
            (60, 11),
            (80, 6),
            (90, 11),
            (200, 30),
            (1, 1),
            (MIN_WIDTH, 5),
        ] {
            for selected in [0usize, 1, 100, 191] {
                let _ = rendered(width, height, selected);
            }
        }
    }

    /// An empty series is a real response shape, and the box still has to be a
    /// box rather than a panic.
    #[test]
    fn an_empty_series_renders_an_empty_box() {
        let mut terminal = Terminal::new(TestBackend::new(60, 11)).unwrap();
        terminal
            .draw(|f| {
                precip_week_render(
                    f,
                    &group_by_day(&[]),
                    palette(),
                    f.area(),
                    Unit::Imperial,
                    0,
                );
            })
            .unwrap();

        let buffer = terminal.backend().buffer().clone();
        let top: String = (0..60).map(|x| buffer[(x, 0)].symbol()).collect();
        assert!(top.contains(TITLE), "{top:?}");
    }
}
