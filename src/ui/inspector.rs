//! The selected-hour inspector both hourly screens draw above their chart: a
//! bordered pane with the city and condition on its top border, the hour and
//! what is coming on its bottom one, a block-digit hero on the left and a
//! column of labelled readings beside it.
//!
//! The two screens differ in the hero and in the readings, and in nothing
//! else. Each supplies those; this module owns the rest, so the two cannot
//! drift apart again. They had: the classic view summed a 24 h total around
//! missing hours that the weathergram's refused to total (#62), ignored an
//! unreported hour when naming the next rain, and clipped the other title
//! first when the border ran short.

use crate::app::App;
use crate::theme::Palette;
use crate::ui::digits::{CELL_WIDTH, DIGIT_ROWS, big_digits};
use crate::ui::pane::{bottom_titles, day_and_hour, long_hour};
use crate::ui::precip_week;
use crate::ui::precipitation::{PrecipitationAggregate, aggregate, measured};
use crate::ui::{TITLE_GUTTER, UNKNOWN, title_room, truncate};
use crate::units::Unit;
use crate::weather::code::description;
use crate::weather::model::HourlyForecast;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

/// Rows the inspector occupies, borders included — the digit block plus the
/// two border rows, so it is sized from the font rather than guessed at.
pub(super) const ROWS: u16 = DIGIT_ROWS as u16 + 2;
/// Matches the daily pane's column. A snow-flagged metric total is the widest
/// value here and needs every one of these; at 30 it clipped mid-word.
pub(super) const DETAIL_WIDTH: u16 = 34;
/// Two border columns around the widest detail line: the least width the
/// pane draws its readings whole in.
pub(super) const WIDTH: u16 = DETAIL_WIDTH + 2;
const COLUMN_GUTTER: u16 = 3;

/// How far ahead the summary rows look. A day is the horizon people actually
/// plan against, and it matches the vertical arrows' jump.
pub(super) const SUMMARY_HOURS: usize = 24;
/// Past this an hour count stops being useful and a date reads better.
const HOURS_BEFORE_A_DATE_READS_BETTER: usize = 24;

/// The big number: three block-digit cells, then a suffix in ordinary text.
/// The suffix decides the hero's width, so `%` and `°F` need no separate
/// constants.
pub(super) struct Hero<'a> {
    pub value: String,
    pub suffix: &'a str,
}

impl Hero<'_> {
    fn width(&self) -> u16 {
        3 * CELL_WIDTH + self.suffix.chars().count() as u16
    }

    fn lines(&self, palette: Palette) -> Vec<Line<'static>> {
        // Alignment::Center centres each line on its own width, so the row
        // carrying the suffix would sit offset. Pad the others to match.
        let pad = " ".repeat(self.suffix.chars().count());
        big_digits(&self.value)
            .iter()
            .enumerate()
            .map(|(i, row)| {
                if i == DIGIT_ROWS / 2 {
                    Line::from(vec![
                        Span::from(row.clone()).bold().fg(palette.accent),
                        Span::from(self.suffix.to_string()).fg(palette.accent),
                    ])
                } else {
                    Line::from(format!("{row}{pad}")).bold().fg(palette.accent)
                }
            })
            .collect()
    }
}

pub(super) fn inspector_render(
    frame: &mut Frame,
    app: &App,
    hours: &[HourlyForecast],
    palette: Palette,
    area: Rect,
    hero: &Hero,
    details: Vec<Line<'static>>,
) {
    let hour = hours.get(app.selected_hour);
    let condition = hour
        .and_then(|h| h.code)
        .map_or(UNKNOWN, description)
        .to_string();
    let when = hour.map_or_else(|| "—".to_string(), |h| long_hour(&h.time));

    // Computed from now rather than from the selection: it is a fact about the
    // world, not about what is highlighted, so it should stay put while you
    // arrow around.
    let upcoming = next_precipitation(hours);

    let (city, condition) = top_titles(&app.location.label, &condition, area.width);
    let (upcoming, when) = bottom_titles(&upcoming, &when, area.width);

    // Titles replace border cells, so a literal space is the only reliable
    // inset across terminal emulators and box-drawing fonts.
    let mut block = Block::bordered()
        .border_style(Style::new().fg(palette.border))
        .title_top(
            Line::from(format!(" {city}"))
                .bold()
                .fg(palette.accent)
                .left_aligned(),
        )
        .title_bottom(
            Line::from(format!("{when} "))
                .fg(palette.text)
                .right_aligned(),
        );

    if let Some(condition) = condition {
        block = block.title_top(
            Line::from(format!("{condition} "))
                .fg(palette.text)
                .right_aligned(),
        );
    }
    if let Some(upcoming) = upcoming {
        // The question people open the app to ask, so it gets the loud colour
        // and the weight rather than the one reserved for labels. It was muted
        // to begin with and read as chrome — the eye went straight past the one
        // line on the screen that answers "do I need a coat".
        block = block.title_bottom(
            Line::from(format!(" {upcoming}"))
                .bold()
                .fg(palette.selection)
                .left_aligned(),
        );
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Same rule as the daily pane: the digits are decoration and the readings
    // are the content, so drop the hero rather than clip it mid-glyph.
    let full = hero.width() + COLUMN_GUTTER + DETAIL_WIDTH;
    let show_hero = inner.width >= full;
    let wanted = if show_hero {
        full
    } else {
        DETAIL_WIDTH.min(inner.width)
    };

    let [content] = Layout::horizontal([Constraint::Length(wanted)])
        .flex(Flex::Center)
        .areas(inner);

    let (hero_area, detail_area) = if show_hero {
        let [hero_area, _gutter, detail] = Layout::horizontal([
            Constraint::Length(hero.width()),
            Constraint::Length(COLUMN_GUTTER),
            Constraint::Length(DETAIL_WIDTH),
        ])
        .areas(content);
        (Some(hero_area), detail)
    } else {
        (None, content)
    };

    if let Some(hero_area) = hero_area {
        frame.render_widget(
            Paragraph::new(hero.lines(palette)).alignment(Alignment::Center),
            hero_area,
        );
    }

    frame.render_widget(Paragraph::new(details), detail_area);
}

/// The week strip's grouping and how many days it may show under a screen
/// that has already spent `reserved` rows, or `None` where it does not fit.
///
/// It is the last thing on the screen to be given rows and the first to give
/// them up: the chart is what the arrows move through, and a strip that
/// squeezed it would cost more than it adds.
///
/// Grouped once, here, for the layout and the strip both. The rows this
/// reserves and the rows the grid draws used to be two separate walks over the
/// series, kept equal by a test; sharing one grouping makes them equal by
/// construction and halves what the screen's most redrawn frame pays for it.
pub(super) fn week_strip(
    hours: &[HourlyForecast],
    area: Rect,
    reserved: u16,
) -> Option<(Vec<precip_week::Day<'_>>, usize)> {
    if area.width < precip_week::MIN_WIDTH + 2 {
        return None;
    }

    let spare = area.height.saturating_sub(reserved);
    let rows = spare.saturating_sub(precip_week::box_rows(0)) as usize;

    // Answered from the geometry alone, like the width check above: `rows` is
    // an upper bound on what the grouping can return, so a terminal too short
    // for the strip would pay for a grouping that is always thrown away.
    if rows < precip_week::MIN_DAYS {
        return None;
    }

    // Calendar dates, not `hours / 24`. A window opening at 6 PM is two and a
    // half days long and touches four dates, and it is dates the strip draws
    // rows for — counting days here reserved one row too few and dropped the
    // tail of the forecast even with the height to show it.
    let days = precip_week::group_by_day(hours);
    let rows = rows.min(days.len());

    (rows >= precip_week::MIN_DAYS).then_some((days, rows))
}

/// The day ahead of the selection, clipped at the end of the forecast.
pub(super) fn window_from(hours: &[HourlyForecast], selected: usize) -> &[HourlyForecast] {
    let end = selected.saturating_add(SUMMARY_HOURS).min(hours.len());
    hours.get(selected..end).unwrap_or_default()
}

/// Snow and rain are different news, so they get different words and different
/// units — a centimetre of snow is not a millimetre of rain.
pub(super) fn amount_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    let Some(hour) = hour else {
        return UNKNOWN.to_string();
    };

    if let Some(cm) = hour.snow_cm.filter(|cm| *cm > 0.0) {
        return format!(
            "{:.*} {} snow",
            unit.snow_decimals(),
            unit.snow(cm),
            unit.snow_label()
        );
    }

    match hour.precip_mm {
        Some(mm) if mm > 0.0 => measured(mm, unit),
        // "none" alone reads as a denial of the chance in the hero beside it;
        // "expected" ties it to the forecast amount, which is what is zero.
        Some(_) => "none expected".to_string(),
        None => UNKNOWN.to_string(),
    }
}

/// Unlike a single hour, a day is very often mixed — rain turning to snow is
/// the ordinary winter case. Reporting only the snow depth would hide the rain
/// that fell with it, so the total is the precipitation total, flagged when
/// some of it arrived frozen. A window with an hour the provider did not
/// report has no total, rather than the sum of the rest passed off as one.
pub(super) fn total_line(ahead: &[HourlyForecast], unit: Unit) -> String {
    let total = aggregate(ahead, unit);
    if total == PrecipitationAggregate::Unavailable {
        return UNKNOWN.to_string();
    }
    let Some(amount) = total.positive_text(unit, " ") else {
        return "none expected".to_string();
    };
    if ahead.iter().any(HourlyForecast::is_snow) {
        return format!("{amount} incl. snow");
    }
    amount
}

/// The question people actually open a weather app to ask. It earns the
/// bottom-left border — the slot the daily pane spends on its period
/// comparison — and costs no interior row.
///
/// Triggered by forecast amount rather than by probability: a 20% hour with no
/// forecast accumulation is not rain, which is the confusion the daily pane
/// already had to correct once. An hour with no amount at all blocks the
/// answer: "no rain in the next 8 days" is a claim about every hour in them.
fn next_precipitation(hours: &[HourlyForecast]) -> String {
    let Some(now) = hours.first() else {
        return String::new();
    };

    match now.precip_mm {
        None => return "precipitation unavailable".to_string(),
        Some(_) if now.is_wet() => return format!("{}ing now", falling_word(now)),
        Some(_) => {}
    }

    for (ahead, hour) in hours.iter().enumerate().skip(1) {
        match hour.precip_mm {
            None => return "precipitation unavailable".to_string(),
            Some(_) if hour.is_wet() && ahead <= HOURS_BEFORE_A_DATE_READS_BETTER => {
                return format!("next {} in {ahead} h", falling_word(hour));
            }
            Some(_) if hour.is_wet() => {
                return format!("next {} {}", falling_word(hour), day_and_hour(&hour.time));
            }
            Some(_) => {}
        }
    }

    format!("no rain in the next {} days", hours.len() / 24)
}

fn falling_word(hour: &HourlyForecast) -> &'static str {
    if hour.is_snow() { "snow" } else { "rain" }
}

/// City left, condition right. At the minimum width the condition is the
/// actionable fact, so the location yields its columns first.
fn top_titles(name: &str, condition: &str, width: u16) -> (String, Option<String>) {
    let available = title_room(width);
    let name = name.to_uppercase();
    let len = |s: &str| s.chars().count();

    if len(&name) + TITLE_GUTTER + len(condition) <= available {
        return (name, Some(condition.to_string()));
    }
    if len(condition) <= available {
        let city_room = available.saturating_sub(len(condition) + TITLE_GUTTER);
        let city = if city_room == 0 {
            String::new()
        } else {
            truncate(&name, city_room)
        };
        return (city, Some(condition.to_string()));
    }
    (String::new(), Some(truncate(condition, available)))
}

/// A dry, mild, breezy series the screens' tests build on. Here rather than
/// in each test module so the two screens are exercised on the same data.
#[cfg(test)]
pub(super) fn dry_hours(count: usize) -> Vec<HourlyForecast> {
    (0..count)
        .map(|i| HourlyForecast {
            time: format!("2026-08-{:02}T{:02}:00", 10 + i / 24, i % 24),
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
        })
        .collect()
}

/// A window that starts in the evening, where an hour count and a count of
/// calendar dates come apart: 60 hours from 6 PM is two and a half days
/// long but touches four dates.
#[cfg(test)]
pub(super) fn evening_hours(count: usize) -> Vec<HourlyForecast> {
    dry_hours(count + 18).split_off(18)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    const CITY: &str = "Frederick, Maryland, United States";

    #[test]
    fn rain_now_is_reported_as_now() {
        let mut hours = dry_hours(48);
        hours[0].precip_mm = Some(1.2);

        assert_eq!(next_precipitation(&hours), "raining now");
    }

    #[test]
    fn snow_gets_its_own_word() {
        let mut hours = dry_hours(48);
        hours[0].precip_mm = Some(1.2);
        hours[0].snow_cm = Some(0.9);
        assert_eq!(next_precipitation(&hours), "snowing now");

        let mut later = dry_hours(48);
        later[3].precip_mm = Some(1.2);
        later[3].snow_cm = Some(0.9);
        assert_eq!(next_precipitation(&later), "next snow in 3 h");
    }

    #[test]
    fn the_next_wet_hour_is_counted_in_hours_while_that_reads_well() {
        let mut hours = dry_hours(96);
        hours[3].precip_mm = Some(0.4);

        assert_eq!(next_precipitation(&hours), "next rain in 3 h");
    }

    /// Past a day an hour count stops meaning anything; a weekday and time is
    /// what someone can actually act on.
    #[test]
    fn a_distant_wet_hour_is_named_by_day() {
        let mut hours = dry_hours(96);
        hours[40].precip_mm = Some(0.4);

        let text = next_precipitation(&hours);
        assert!(text.starts_with("next rain "), "{text:?}");
        assert!(
            text.contains("Tue"),
            "40 h past Mon 10 Aug is Tue: {text:?}"
        );
        assert!(!text.contains(" h"), "no bare hour count that far out");
    }

    /// The common case, and the one the screen must not render as silence.
    #[test]
    fn a_dry_window_says_how_long_it_stays_dry() {
        assert_eq!(
            next_precipitation(&dry_hours(192)),
            "no rain in the next 8 days"
        );
    }

    /// Probability alone must not trigger it: an hour with a 90% chance and no
    /// forecast accumulation is not rain.
    #[test]
    fn a_high_chance_with_no_amount_is_not_rain() {
        let mut hours = dry_hours(48);
        hours[5].chance = Some(90);

        assert_eq!(next_precipitation(&hours), "no rain in the next 2 days");
    }

    #[test]
    fn an_empty_series_says_nothing_rather_than_guessing() {
        assert_eq!(next_precipitation(&[]), "");
    }

    #[test]
    fn an_unknown_amount_blocks_dry_and_later_event_claims() {
        let mut later_rain = dry_hours(48);
        later_rain[1].precip_mm = None;
        later_rain[3].precip_mm = Some(0.4);
        assert_eq!(next_precipitation(&later_rain), "precipitation unavailable");

        let mut otherwise_dry = dry_hours(48);
        otherwise_dry[1].precip_mm = None;
        assert_eq!(
            next_precipitation(&otherwise_dry),
            "precipitation unavailable"
        );
    }

    /// A positive amount rounding to zero at the display precision would read
    /// as a dry hour. 0.1 mm is 0.0039 in — the case that actually occurs.
    #[test]
    fn a_trace_amount_never_renders_as_zero() {
        let mut hours = dry_hours(4);
        hours[0].precip_mm = Some(0.1);

        assert_eq!(amount_line(hours.first(), Unit::Imperial), "<0.01 in");
        assert_eq!(amount_line(hours.first(), Unit::Metric), "0.1 mm");

        hours[0].precip_mm = Some(0.01);
        assert_eq!(amount_line(hours.first(), Unit::Metric), "<0.1 mm");
    }

    #[test]
    fn a_measured_amount_renders_at_the_units_precision() {
        let mut hours = dry_hours(4);
        hours[0].precip_mm = Some(25.4);

        assert_eq!(amount_line(hours.first(), Unit::Imperial), "1.00 in");
        assert_eq!(amount_line(hours.first(), Unit::Metric), "25.4 mm");
    }

    #[test]
    fn an_exactly_dry_hour_says_none_expected() {
        let hours = dry_hours(4);
        assert_eq!(amount_line(hours.first(), Unit::Imperial), "none expected");
    }

    #[test]
    fn a_missing_amount_is_unavailable_rather_than_zero() {
        let mut hours = dry_hours(4);
        hours[0].precip_mm = None;
        hours[0].snow_cm = None;

        assert_eq!(amount_line(hours.first(), Unit::Metric), UNKNOWN);
        assert_eq!(amount_line(None, Unit::Metric), UNKNOWN);
    }

    #[test]
    fn snow_is_measured_in_its_own_units() {
        let mut hours = dry_hours(4);
        hours[0].snow_cm = Some(2.5);
        hours[0].precip_mm = Some(1.8);

        assert_eq!(amount_line(hours.first(), Unit::Metric), "2.5 cm snow");
        assert_eq!(amount_line(hours.first(), Unit::Imperial), "1.0 in snow");
    }

    #[test]
    fn selected_forward_totals_reject_partial_measurements() {
        let mut hours = dry_hours(24);
        hours[7].precip_mm = None;

        assert_eq!(total_line(&hours, Unit::Metric), UNKNOWN);
    }

    #[test]
    fn selected_forward_totals_distinguish_zero_and_trace() {
        let zero = dry_hours(24);
        assert_eq!(total_line(&zero, Unit::Metric), "none expected");

        let mut trace = dry_hours(24);
        trace[0].precip_mm = Some(0.01);
        assert_eq!(total_line(&trace, Unit::Metric), "<0.1 mm");
        assert_eq!(total_line(&trace, Unit::Imperial), "<0.01 in");
    }

    #[test]
    fn a_snowy_window_flags_its_total() {
        let mut hours = dry_hours(24);
        hours[2].precip_mm = Some(3.0);
        hours[2].snow_cm = Some(2.0);

        assert_eq!(total_line(&hours, Unit::Metric), "3.0 mm incl. snow");
    }

    /// The window must shorten rather than run off the end of the series.
    #[test]
    fn the_summary_window_is_clipped_at_the_end_of_the_forecast() {
        let hours = dry_hours(30);
        assert_eq!(window_from(&hours, 20).len(), 10);
        assert_eq!(window_from(&hours, 29).len(), 1);
        assert!(window_from(&hours, 30).is_empty());
    }

    #[test]
    fn the_daily_total_adds_the_window_up() {
        let mut hours = dry_hours(48);
        for hour in hours.iter_mut().take(4) {
            hour.precip_mm = Some(6.35);
        }

        assert_eq!(
            total_line(window_from(&hours, 0), Unit::Imperial),
            "1.00 in"
        );
        assert_eq!(
            total_line(window_from(&hours, 24), Unit::Imperial),
            "none expected"
        );
    }

    /// The suffix sets the hero's width and the padding on the rows that do
    /// not carry it, so `%` and `°F` heroes both centre on their digits.
    #[test]
    fn the_hero_is_sized_by_its_suffix() {
        let chance = Hero {
            value: "53".to_string(),
            suffix: "%",
        };
        let temperature = Hero {
            value: "33".to_string(),
            suffix: "°F",
        };
        assert_eq!(chance.width(), 3 * CELL_WIDTH + 1);
        assert_eq!(temperature.width(), 3 * CELL_WIDTH + 2);

        let palette = Theme::default().palette();
        let lines = temperature.lines(palette);
        assert_eq!(lines.len(), DIGIT_ROWS);
        assert!(lines[DIGIT_ROWS / 2].to_string().ends_with("°F"));
        let widths: Vec<usize> = lines.iter().map(Line::width).collect();
        assert!(
            widths.iter().all(|w| *w == widths[0]),
            "the rows do not share a width: {widths:?}"
        );
    }

    #[test]
    fn the_condition_is_preserved_before_a_long_location() {
        let (city, condition) = top_titles(CITY, "Light rain", 100);
        assert_eq!(city, CITY.to_uppercase());
        assert_eq!(condition.as_deref(), Some("Light rain"));

        let (city, condition) = top_titles(CITY, "Thunderstorm, heavy hail", 48);
        assert!(
            city.ends_with('…'),
            "the location should give way: {city:?}"
        );
        assert_eq!(condition.as_deref(), Some("Thunderstorm, heavy hail"));

        let (city, condition) = top_titles(CITY, "Thunderstorm, heavy hail", 34);
        assert!(
            city.ends_with('…'),
            "the minimum should clip location: {city:?}"
        );
        assert_eq!(condition.as_deref(), Some("Thunderstorm, heavy hail"));
    }

    /// Both border rows carry two titles on the same line, so neither pair may
    /// overlap at any width the app renders at.
    #[test]
    fn border_titles_never_collide() {
        for width in 20u16..=200 {
            let available = title_room(width);

            let (city, condition) = top_titles(CITY, "Thunderstorm, heavy hail", width);
            let used = city.chars().count() + condition.map_or(0, |c| c.chars().count());
            assert!(used <= available, "top at {width}: {used} of {available}");

            let when = "Mon 10 Aug, 4:00 PM";
            let (upcoming, when) = bottom_titles("no rain in the next 8 days", when, width);
            let used = when.chars().count() + upcoming.map_or(0, |u| u.chars().count());
            assert!(
                used <= available || available < when.chars().count(),
                "bottom at {width}: {used} of {available}"
            );
        }
    }
}
