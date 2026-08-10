use crate::app::App;
use crate::ui::UNKNOWN;
use crate::ui::digits::{CELL_WIDTH, DIGIT_ROWS, big_digits};
use crate::units::Unit;
use crate::weather::code::{aqi_label, description};
use crate::weather::model::{DailyForecast, Weather};
use chrono::NaiveDate;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

pub(super) fn current_area_render(frame: &mut Frame, app: &App, weather: &Weather, area: Rect) {
    let unit = app.unit;
    let selected = app.selected_day;
    // The pane doubles as the day inspector. On today it shows live current
    // conditions; on any other day it shows that day's summary, so arrowing
    // through the chart has somewhere to put its answer.
    let day = weather.daily.get(selected);
    let showing_today = selected == weather.today_index || day.is_none();

    let when = if showing_today {
        "Today".to_string()
    } else {
        day.map_or_else(|| "Today".to_string(), |d| long_date(&d.date))
    };

    let name = app.location.as_deref().unwrap_or(&weather.location);

    let condition = if showing_today {
        weather.current.code.map_or(UNKNOWN, description)
    } else {
        day.map_or(UNKNOWN, |d| description(d.code))
    };

    // The period comparison earns its place on the border rather than in the
    // detail column: it is a sentence, not a reading, and moving it there frees
    // an interior row. Nothing stops two titles on the same row overwriting
    // each other, so each border gets an explicit budget.
    let summary = day.map_or_else(String::new, |d| comparison(weather, d, unit));

    // Air quality rides the border beside the condition, where it costs no
    // rows. Today shows the live reading; other days show that day's worst,
    // derived from the hourly series. Days past the endpoint's horizon simply
    // have none, and the border omits it rather than showing a placeholder.
    let aqi = if showing_today {
        weather.air_quality.as_ref().map(|aq| aq.us_aqi)
    } else {
        day.and_then(|d| d.aqi)
    }
    .map(|value| format!("AQI {value} {}", aqi_label(value)));

    let (city, condition, aqi) = top_titles(name, condition, aqi.as_deref(), area.width);
    let (summary, when) = bottom_titles(&summary, &when, area.width);

    let mut block = Block::bordered()
        .title_top(Line::from(city).bold().blue().left_aligned())
        .title_bottom(Line::from(when).white().right_aligned());

    if let Some(condition) = condition {
        // The rule between them is left unstyled so it takes the border's own
        // colour, and the border appears to run behind the text.
        let mut right = vec![Span::from(condition).white()];
        if let Some(aqi) = aqi {
            right.push(Span::from(TITLE_RULE));
            right.push(Span::from(aqi).white());
        }
        block = block.title_top(Line::from(right).right_aligned());
    }
    if let Some(summary) = summary {
        block = block.title_bottom(Line::from(summary).dark_gray().left_aligned());
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The block digits are decoration; the readings are the content. Rather
    // than squeeze both and clip the digits mid-glyph, drop the hero entirely
    // once there is not room for the pair.
    let full = HERO_WIDTH + COLUMN_GUTTER + DETAIL_WIDTH;
    let show_hero = inner.width >= full;

    let wanted = if show_hero {
        full
    } else {
        DETAIL_WIDTH.min(inner.width)
    };

    // Centre the group rather than pinning it left.
    let [content] = Layout::horizontal([Constraint::Length(wanted)])
        .flex(Flex::Center)
        .areas(inner);

    // Without a gutter a three-digit temperature fills HERO_WIDTH exactly and
    // its unit symbol lands flush against the first label.
    let (hero_area, detail_area) = if show_hero {
        let [hero, _gutter, detail] = Layout::horizontal([
            Constraint::Length(HERO_WIDTH),
            Constraint::Length(COLUMN_GUTTER),
            Constraint::Length(DETAIL_WIDTH),
        ])
        .areas(content);
        (Some(hero), detail)
    } else {
        (None, content)
    };

    // The block font already has a '-' glyph, so a missing reading renders as
    // "--" at the same scale rather than collapsing the layout.
    let temp = if showing_today {
        weather
            .current
            .temp_c
            .map_or_else(|| "--".to_string(), |c| format!("{:.0}", unit.temp(c)))
    } else {
        day.map_or_else(
            || "--".to_string(),
            |d| format!("{:.0}", unit.temp(d.high_c)),
        )
    };

    // Alignment::Center centres each line on its own width, so the row carrying
    // the unit symbol would sit offset from the rest. Pad the others to match.
    let symbol = unit.temp_symbol();
    let symbol_pad = " ".repeat(symbol.chars().count());

    let hero: Vec<Line> = big_digits(&temp)
        .iter()
        .enumerate()
        .map(|(i, row)| {
            // Hang the unit symbol off the middle row so it sits centred against the digits.
            if i == DIGIT_ROWS / 2 {
                Line::from(vec![
                    Span::from(row.clone()).bold().blue(),
                    Span::from(symbol.to_string()).blue(),
                ])
            } else {
                Line::from(format!("{row}{symbol_pad}")).bold().blue()
            }
        })
        .collect();
    if let Some(hero_area) = hero_area {
        frame.render_widget(Paragraph::new(hero).alignment(Alignment::Center), hero_area);
    }

    // Both branches produce the same number of lines so today is no thinner
    // than any other day. Today swaps the period comparison for air quality,
    // which only exists for now; other days swap the live reading for the
    // day's feels-like range.
    let details = day.map_or_else(Vec::new, |d| detail_lines(weather, d, unit, showing_today));

    frame.render_widget(Paragraph::new(details), detail_area);
}

/// Widths of the two columns in the current pane, centred as a pair. The hero
/// holds three digits and the unit symbol, sized from the font rather than
/// restated here.
const HERO_WIDTH: u16 = 3 * CELL_WIDTH + 2;
const DETAIL_WIDTH: u16 = 30;
/// Columns between the hero and the details.
const COLUMN_GUTTER: u16 = 3;
/// Columns kept clear between the two border titles.
const TITLE_GUTTER: usize = 3;
/// Joins the condition to the air quality. Box-drawing horizontals, so at the
/// border's own colour the rule reads as running behind the text.
const TITLE_RULE: &str = "───";

/// One builder for both cases: the only difference is where "feels like" comes
/// from — a live reading today, the day's range otherwise. Everything else is
/// the same daily figure, so the two can no longer drift apart in length.
fn detail_lines(
    weather: &Weather,
    day: &DailyForecast,
    unit: Unit,
    showing_today: bool,
) -> Vec<Line<'static>> {
    let sym = unit.temp_symbol();

    let feels = if showing_today {
        weather.current.feels_like_c.map_or_else(
            || UNKNOWN.to_string(),
            |c| format!("{:.0}{sym}", unit.temp(c)),
        )
    } else {
        match (day.feels_max_c, day.feels_min_c) {
            (Some(hi), Some(lo)) => {
                format!("{:.0}{sym} / {:.0}{sym}", unit.temp(hi), unit.temp(lo))
            }
            _ => UNKNOWN.to_string(),
        }
    };

    let wind = if showing_today {
        wind_line(
            day.wind_kph.or(weather.current.wind_kph),
            day.gust_kph,
            day,
            unit,
        )
    } else {
        wind_line(day.wind_kph, day.gust_kph, day, unit)
    };

    vec![
        detail_line("feels like", &feels),
        detail_line("high / low", &high_low(day, unit)),
        detail_line("rain", &rain_line(day, unit)),
        detail_line("wind", &wind),
        detail_line("daylight", &daylight_line(day)),
    ]
}

fn high_low(day: &DailyForecast, unit: Unit) -> String {
    let sym = unit.temp_symbol();
    format!(
        "{:.0}{sym} / {:.0}{sym}",
        unit.temp(day.high_c),
        unit.temp(day.low_c)
    )
}

fn rain_line(day: &DailyForecast, unit: Unit) -> String {
    match (day.precip_mm, day.precip_hours) {
        (Some(mm), Some(h)) if mm > 0.0 => format!(
            "{:.2} {} over {:.0} h",
            unit.precip(mm),
            unit.precip_label(),
            h
        ),
        (Some(_), _) => "none".to_string(),
        _ => UNKNOWN.to_string(),
    }
}

fn wind_line(speed: Option<f64>, gust: Option<f64>, day: &DailyForecast, unit: Unit) -> String {
    let direction = day
        .wind_dir_deg
        .map_or(String::new(), |d| format!(" {}", compass(d)));

    match (speed, gust) {
        (Some(w), Some(g)) => format!(
            "{:.0}, gusts {:.0} {}{direction}",
            unit.speed(w),
            unit.speed(g),
            unit.speed_label(),
        ),
        (Some(w), None) => format!("{:.0} {}{direction}", unit.speed(w), unit.speed_label()),
        _ => UNKNOWN.to_string(),
    }
}

fn daylight_line(day: &DailyForecast) -> String {
    day.daylight_secs
        .map_or_else(|| UNKNOWN.to_string(), duration)
}

/// The part a table cannot give you: whether the day is remarkable for the
/// period, not merely what its number is.
fn comparison(weather: &Weather, day: &DailyForecast, unit: Unit) -> String {
    if weather.daily.is_empty() {
        return String::new();
    }

    let mean = weather.daily.iter().map(|d| d.high_c).sum::<f64>() / weather.daily.len() as f64;
    let delta = unit.temp(day.high_c) - unit.temp(mean);

    if delta.abs() < 1.0 {
        "about average for the period".to_string()
    } else {
        format!(
            "{:.0}{} {} the {}-day average",
            delta.abs(),
            unit.temp_symbol(),
            if delta > 0.0 { "above" } else { "below" },
            weather.daily.len(),
        )
    }
}

/// Sixteen points is more precision than a daily dominant direction deserves.
fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    POINTS[(((degrees % 360.0 + 360.0) % 360.0 / 45.0).round() as usize) % 8]
}

fn duration(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64 / 60;
    format!("{}h {}m", total / 60, total % 60)
}

fn long_date(date: &str) -> String {
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(parsed) => parsed.format("%a, %b %-d").to_string(),
        Err(_) => date.to_string(),
    }
}

/// Fits the city, condition and day into the border, dropping detail rather
/// than letting the two titles overwrite each other. The day matters most —
/// it is what changes as you arrow around — then the city, then the condition.
/// Two corners, plus a column of breathing room inside each.
fn title_room(width: u16) -> usize {
    width.saturating_sub(4) as usize
}

/// City left; condition and air quality right. The city is the identity, so it
/// is clipped last; air quality is supplementary, so it goes first.
fn top_titles(
    name: &str,
    condition: &str,
    aqi: Option<&str>,
    width: u16,
) -> (String, Option<String>, Option<String>) {
    let available = title_room(width);
    let name = name.to_uppercase();
    let len = |s: &str| s.chars().count();
    let city = len(&name) + TITLE_GUTTER;

    if let Some(aqi) = aqi {
        let right = len(condition) + TITLE_RULE.chars().count() + len(aqi);
        if city + right <= available {
            return (name, Some(condition.to_string()), Some(aqi.to_string()));
        }
    }

    if city + len(condition) <= available {
        return (name, Some(condition.to_string()), None);
    }

    if len(&name) <= available {
        return (name, None, None);
    }

    (truncate(&name, available), None, None)
}

/// Comparison left, day right. The day is what changes as you arrow around, so
/// it is never the one sacrificed.
fn bottom_titles(summary: &str, when: &str, width: u16) -> (Option<String>, String) {
    let available = title_room(width);
    let len = |s: &str| s.chars().count();

    if !summary.is_empty() && len(summary) + TITLE_GUTTER + len(when) <= available {
        return (Some(summary.to_string()), when.to_string());
    }

    (None, when.to_string())
}

/// Clip to `width` on a character boundary, marking the cut so a truncated
/// value cannot be mistaken for a short one.
fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    text.chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}

fn detail_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<12}")).dark_gray(),
        Span::from(value.to_string()).white(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Fetch;

    use crate::units::Unit;
    use crate::weather::model::Weather;

    /// Today used to carry fewer lines than any other day, which read as a gap
    /// rather than a difference. They must also match the digit block's row
    /// count, or the hero cannot sit level with them.
    #[test]
    fn both_branches_produce_one_line_per_digit_row() {
        let w = Weather::fixture(22, 14);

        for (day, today) in [(&w.daily[14], true), (&w.daily[3], false)] {
            assert_eq!(
                detail_lines(&w, day, Unit::Imperial, today).len(),
                DIGIT_ROWS,
                "today = {today}"
            );
        }
    }

    #[test]
    fn comparison_names_the_direction_from_the_average() {
        let w = Weather::fixture(21, 10);
        // Fixture highs climb 20..40, so the mean is the middle day.
        assert!(comparison(&w, &w.daily[20], Unit::Metric).contains("above"));
        assert!(comparison(&w, &w.daily[0], Unit::Metric).contains("below"));
        assert!(comparison(&w, &w.daily[10], Unit::Metric).contains("average"));
    }

    #[test]
    fn comparison_is_empty_rather_than_dividing_by_zero() {
        let w = Weather::fixture(0, 0);
        let day = Weather::fixture(1, 0).daily.remove(0);
        assert_eq!(comparison(&w, &day, Unit::Metric), "");
    }

    #[test]
    fn missing_readings_read_as_unavailable_rather_than_blank() {
        let mut w = Weather::fixture(3, 1);
        let day = &mut w.daily[1];
        day.precip_mm = None;
        day.gust_kph = None;
        day.wind_kph = None;
        day.daylight_secs = None;

        assert_eq!(rain_line(&w.daily[1], Unit::Metric), UNKNOWN);
        assert_eq!(daylight_line(&w.daily[1]), UNKNOWN);
        assert_eq!(wind_line(None, None, &w.daily[1], Unit::Metric), UNKNOWN);
    }

    const CITY: &str = "Frederick, Maryland, United States";

    const AQI: &str = "AQI 54 Moderate";

    #[test]
    fn top_shows_condition_and_air_quality_when_there_is_room() {
        let (city, condition, aqi) = top_titles(CITY, "Drizzle", Some(AQI), 120);
        assert_eq!(city, CITY.to_uppercase());
        assert_eq!(condition.as_deref(), Some("Drizzle"));
        assert_eq!(aqi.as_deref(), Some(AQI));
    }

    /// Air quality is supplementary, so it is the first thing to go.
    #[test]
    fn top_drops_air_quality_before_the_condition() {
        let (city, condition, aqi) = top_titles(CITY, "Drizzle", Some(AQI), 62);
        assert_eq!(city, CITY.to_uppercase());
        assert_eq!(condition.as_deref(), Some("Drizzle"), "condition survived");
        assert_eq!(aqi, None);
    }

    /// The city is the identity, so the condition goes before it is clipped.
    #[test]
    fn top_drops_the_condition_before_clipping_the_city() {
        let (city, condition, aqi) = top_titles(CITY, "Thunderstorm, heavy hail", Some(AQI), 48);
        assert_eq!(city, CITY.to_uppercase(), "the city stayed whole");
        assert_eq!(condition, None);
        assert_eq!(aqi, None);
    }

    #[test]
    fn top_clips_the_city_only_as_a_last_resort() {
        let (city, condition, _) = top_titles(CITY, "Drizzle", Some(AQI), 24);
        assert!(city.ends_with('…'), "the city took the cut: {city:?}");
        assert_eq!(condition, None);
    }

    /// Days beyond the endpoint's horizon have no reading at all.
    #[test]
    fn top_omits_air_quality_when_there_is_none() {
        let (_, condition, aqi) = top_titles(CITY, "Drizzle", None, 120);
        assert_eq!(condition.as_deref(), Some("Drizzle"));
        assert_eq!(aqi, None);
    }

    /// The day is what changes as you arrow around, so it is never sacrificed.
    #[test]
    fn bottom_keeps_the_day_and_drops_the_comparison() {
        let long = "17°F above the 22-day average";

        let (summary, when) = bottom_titles(long, "Fri, Aug 21", 120);
        assert_eq!(summary.as_deref(), Some(long));
        assert_eq!(when, "Fri, Aug 21");

        let (summary, when) = bottom_titles(long, "Fri, Aug 21", 40);
        assert_eq!(summary, None);
        assert_eq!(when, "Fri, Aug 21");
    }

    #[test]
    fn bottom_omits_an_empty_comparison() {
        let (summary, _) = bottom_titles("", "Today", 120);
        assert_eq!(summary, None);
    }

    /// Both border rows carry two titles drawn onto the same line, so neither
    /// pair may overlap at any width the app renders at.
    #[test]
    fn border_titles_never_collide() {
        for width in 20u16..=200 {
            let available = title_room(width);

            let (city, condition, aqi) =
                top_titles(CITY, "Thunderstorm, heavy hail", Some(AQI), width);
            let used = city.chars().count()
                + condition.map_or(0, |c| c.chars().count())
                + aqi.map_or(0, |a| a.chars().count() + TITLE_RULE.chars().count());
            assert!(used <= available, "top at {width}: {used} of {available}");

            let (summary, when) =
                bottom_titles("17°F above the 22-day average", "Fri, Aug 21", width);
            let used = when.chars().count() + summary.map_or(0, |s| s.chars().count());
            assert!(
                used <= available || summary_fits_nowhere(width),
                "bottom at {width}: {used} of {available}"
            );
        }
    }

    /// Below a certain width even the day alone exceeds the border.
    fn summary_fits_nowhere(width: u16) -> bool {
        title_room(width) < "Fri, Aug 21".len()
    }

    /// The budget is arithmetic; this checks what ratatui actually draws.
    #[test]
    fn rendered_border_keeps_the_day_and_never_overlaps() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.selected_day = 20;
        app.location = Some(CITY.to_string());

        for width in [44u16, 60, 80, 120, 200] {
            let mut t = Terminal::new(TestBackend::new(width, 9)).unwrap();
            t.draw(|f| current_area_render(f, &app, &Weather::fixture(22, 14), f.area()))
                .unwrap();

            let buf = t.backend().buffer();
            let row = |y: u16| -> String { (0..width).map(|x| buf[(x, y)].symbol()).collect() };
            let (top, bottom) = (row(0), row(8));

            assert!(
                bottom.contains("Aug"),
                "width {width}: day missing from bottom border {bottom:?}"
            );
            assert!(
                top.contains("FREDERICK"),
                "width {width}: city missing from top border {top:?}"
            );
            // Titles running together is what a collision looks like once
            // rendered; the border rule should always separate them.
            for line in [&top, &bottom] {
                assert!(
                    line.contains('─'),
                    "width {width}: no rule left between titles in {line:?}"
                );
            }
        }
    }

    #[test]
    fn truncate_marks_the_cut_and_leaves_short_text_alone() {
        assert_eq!(truncate("Clear sky", 26), "Clear sky");
        assert_eq!(
            truncate("Thunderstorm, heavy hail", 26),
            "Thunderstorm, heavy hail"
        );
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("", 4), "");
    }

    /// Narrow panes drop the block digits rather than clipping them mid-glyph.
    /// Whatever happens to the decoration, every reading must survive intact.
    #[test]
    fn narrow_panes_keep_the_readings_and_drop_the_digits() {
        use crate::app::App;
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(22, 14));
        app.location = Some(CITY.to_string());

        for width in [34u16, 44, 58, 59, 80] {
            let mut t = Terminal::new(TestBackend::new(width, 9)).unwrap();
            t.draw(|f| current_area_render(f, &app, &Weather::fixture(22, 14), f.area()))
                .unwrap();

            let buf = t.backend().buffer();
            let text: String = (0..9u16)
                .flat_map(|y| (0..width).map(move |x| (x, y)))
                .map(|(x, y)| buf[(x, y)].symbol())
                .collect();

            for label in ["feels like", "high / low", "daylight"] {
                assert!(text.contains(label), "width {width} lost {label:?}");
            }
        }
    }

    #[test]
    fn compass_maps_degrees_to_points() {
        assert_eq!(compass(0.0), "N");
        assert_eq!(compass(45.0), "NE");
        assert_eq!(compass(90.0), "E");
        assert_eq!(compass(180.0), "S");
        assert_eq!(compass(315.0), "NW");
    }

    /// 360 must not index past the end of the table, and the API has been known
    /// to report a hair over or under.
    #[test]
    fn compass_wraps_at_the_full_circle() {
        assert_eq!(compass(360.0), "N");
        assert_eq!(compass(359.0), "N");
        assert_eq!(compass(720.0), "N");
        assert_eq!(compass(-45.0), "NW");
    }

    #[test]
    fn duration_splits_seconds_into_hours_and_minutes() {
        assert_eq!(duration(49_320.0), "13h 42m");
        assert_eq!(duration(3_600.0), "1h 0m");
        assert_eq!(duration(0.0), "0h 0m");
        assert_eq!(duration(-5.0), "0h 0m");
    }

    #[test]
    fn long_date_falls_back_to_the_raw_value() {
        assert_eq!(long_date("2026-08-11"), "Tue, Aug 11");
        assert_eq!(long_date("not-a-date"), "not-a-date");
    }
}
