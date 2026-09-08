//! The `virga now` report: current conditions and today's outlook as a few
//! lines of plain text.
//!
//! This lives apart from `main` so the report can be built from a fixture and
//! read in a test, with no network and no terminal. Nothing here is
//! Ratatui's: the report goes to stdout, where a script or a status bar reads
//! it as easily as a person does, so it is plain lines — no colour, no boxes,
//! no alternate screen.

use crate::app::{ActiveLocation, Remembered};
use crate::units::Unit;
use crate::weather::code;
use crate::weather::model::Weather;
use chrono::{DateTime, FixedOffset};

/// Where the report should ask about, given what the state file remembered
/// and whether the IP lookup is allowed.
///
/// A remembered place answers outright, whoever put it there. The full
/// interface re-detects over a remembered *detection* on every launch; a
/// one-shot must not, because a status bar polling once a minute would spend
/// the location provider's whole daily allowance before dinner, and
/// yesterday's city is a fine answer to a question this casual. Detection
/// runs only when nothing is remembered at all — and the caller remembers
/// what it finds, so it runs once, not once per poll.
#[derive(Debug, PartialEq)]
pub(crate) enum Ask {
    /// Fetch for this place directly.
    Location(ActiveLocation),
    /// Ask the network where we are first, and settle for `fallback` when it
    /// does not answer — a worse guess is not a reason to withhold the
    /// weather.
    Detect { fallback: ActiveLocation },
}

pub(crate) fn where_to_ask(remembered: Option<Remembered>, detect: bool) -> Ask {
    match remembered {
        Some(Remembered { location, .. }) => Ask::Location(location),
        None if detect => Ask::Detect {
            fallback: ActiveLocation::default(),
        },
        None => Ask::Location(ActiveLocation::default()),
    }
}

/// The report: the place and its sky, the conditions this hour, and today's
/// outlook. Every reading is optional in the model and stays optional here —
/// a missing measurement vanishes rather than printing a dash, because a
/// script grepping the output wants absent things absent. A line with
/// nothing left to say is dropped whole; only the place always prints.
///
/// `now` is the machine's clock with its own offset: what decides whether
/// the place's local time is worth a mention, and what it is computed from.
pub(crate) fn report(
    label: &str,
    weather: &Weather,
    unit: Unit,
    now: DateTime<FixedOffset>,
) -> String {
    let mut lines = vec![headline(label, weather, now)];
    lines.extend(conditions(weather, unit));
    lines.extend(outlook(weather, unit));
    lines.join("\n")
}

/// The place, its sky, and — for a place whose clocks disagree with the
/// machine's — the time there. On the first line rather than its own, so a
/// status bar that takes the report a line at a time gets the same lines
/// for Tokyo as for the city it sits in.
fn headline(label: &str, weather: &Weather, now: DateTime<FixedOffset>) -> String {
    let mut parts = vec![label.to_string()];
    if let Some(code) = weather.current.code {
        parts.push(code::description(code).to_string());
    }
    if let Some(there) = local_time(weather, now) {
        parts.push(there);
    }
    parts.join(" · ")
}

/// The clock at the place, from the offset the forecast response carried,
/// which is the same source the cache trusts and needs no zone database.
/// Nothing when the offset is unknown, or when it matches the machine's:
/// two zones that read the same clock right now have nothing to tell.
fn local_time(weather: &Weather, now: DateTime<FixedOffset>) -> Option<String> {
    let offset = weather.utc_offset_secs.and_then(FixedOffset::east_opt)?;
    if offset == *now.offset() {
        return None;
    }
    let there = now.with_timezone(&offset);
    Some(format!(
        "local time {} (UTC{offset})",
        there.format("%H:%M")
    ))
}

/// This hour: temperature, feels-like, wind, and the live air quality.
fn conditions(weather: &Weather, unit: Unit) -> Option<String> {
    let sym = unit.temp_symbol();
    let current = &weather.current;
    let mut parts = Vec::new();
    match (current.temp_c, current.feels_like_c) {
        (Some(temp), Some(feels)) => parts.push(format!(
            "{:.0}{sym}, feels like {:.0}{sym}",
            unit.temp_rounded(temp),
            unit.temp_rounded(feels)
        )),
        (Some(temp), None) => parts.push(format!("{:.0}{sym}", unit.temp_rounded(temp))),
        (None, Some(feels)) => {
            parts.push(format!("feels like {:.0}{sym}", unit.temp_rounded(feels)))
        }
        (None, None) => {}
    }
    if let Some(wind) = current.wind_kph {
        parts.push(format!(
            "wind {:.0} {}",
            unit.speed(wind),
            unit.speed_label()
        ));
    }
    if let Some(aqi) = &weather.air_quality {
        parts.push(format!(
            "AQI {} {}",
            aqi.us_aqi,
            code::aqi_label(aqi.us_aqi)
        ));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Today's line: the high and low, rain, UV, and the sun's hours.
fn outlook(weather: &Weather, unit: Unit) -> Option<String> {
    let day = weather.daily.get(weather.today_index)?;
    let sym = unit.temp_symbol();
    let mut parts = vec![format!(
        "{:.0}{sym} / {:.0}{sym}",
        unit.temp_rounded(day.high_c),
        unit.temp_rounded(day.low_c)
    )];
    // The chance is the number people plan around; the amount steps in only
    // when the chance is missing and something is actually forecast to fall.
    match (day.rain_chance, day.precip_mm) {
        (Some(chance), _) => parts.push(format!("rain {chance}%")),
        (None, Some(mm)) if mm > 0.0 => parts.push(format!(
            "rain {:.*} {}",
            unit.precip_decimals(),
            unit.precip(mm),
            unit.precip_label()
        )),
        _ => {}
    }
    if let Some(uv) = day.uv_index {
        parts.push(format!("UV {uv:.0}"));
    }
    if let (Some(rise), Some(set)) = (
        day.sunrise.as_deref().and_then(clock_time),
        day.sunset.as_deref().and_then(clock_time),
    ) {
        parts.push(format!("sun {rise}–{set}"));
    }
    Some(format!("Today: {}", parts.join(" · ")))
}

/// The clock part of an ISO timestamp — "2026-08-09T06:17" reads "06:17".
fn clock_time(stamp: &str) -> Option<&str> {
    stamp.split_once('T').map(|(_, time)| time)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::LocationSource;
    use crate::weather::model::{AirQuality, Current, Weather};

    fn berlin() -> ActiveLocation {
        ActiveLocation {
            label: "Berlin, Germany".to_string(),
            lat: 52.52437,
            lon: 13.41053,
        }
    }

    /// The machine's clock, at an offset the fixture never carries, so the
    /// reports below stay about the weather.
    fn here() -> DateTime<FixedOffset> {
        "2026-08-03T14:30:00+02:00"
            .parse()
            .expect("a well-formed stamp")
    }

    fn remembered(source: LocationSource) -> Remembered {
        Remembered {
            location: berlin(),
            source,
        }
    }

    #[test]
    fn a_remembered_choice_is_asked_about_directly() {
        assert_eq!(
            where_to_ask(Some(remembered(LocationSource::Chosen)), true),
            Ask::Location(berlin())
        );
    }

    /// The interface re-detects over a remembered detection; the one-shot
    /// must not, or a status bar polling by the minute would spend the
    /// location provider's daily allowance asking the same question.
    #[test]
    fn a_remembered_detection_is_not_re_detected() {
        assert_eq!(
            where_to_ask(Some(remembered(LocationSource::Detected)), true),
            Ask::Location(berlin())
        );
    }

    #[test]
    fn nothing_remembered_detects_with_the_builtin_fallback() {
        assert_eq!(
            where_to_ask(None, true),
            Ask::Detect {
                fallback: ActiveLocation::default()
            }
        );
    }

    #[test]
    fn opting_out_of_detection_asks_about_the_fallback_directly() {
        assert_eq!(
            where_to_ask(None, false),
            Ask::Location(ActiveLocation::default())
        );
    }

    #[test]
    fn the_full_report_in_metric() {
        let weather = Weather::fixture(5, 2);

        assert_eq!(
            report("Berlin, Germany", &weather, Unit::Metric, here()),
            "Berlin, Germany · Clear sky\n\
             25°C, feels like 26°C · wind 10 km/h\n\
             Today: 22°C / 12°C · rain 10% · UV 6 · sun 06:00–20:00"
        );
    }

    /// Every measure converts, not just the temperature — the wind staying in
    /// km/h beside a °F reading is the mistake this pins down.
    #[test]
    fn the_full_report_in_imperial() {
        let weather = Weather::fixture(5, 2);

        assert_eq!(
            report("Berlin, Germany", &weather, Unit::Imperial, here()),
            "Berlin, Germany · Clear sky\n\
             77°F, feels like 79°F · wind 6 mph\n\
             Today: 72°F / 54°F · rain 10% · UV 6 · sun 06:00–20:00"
        );
    }

    /// A reading in [-0.5, 0] used to print as `-0°`, which no thermometer
    /// says and which a script parsing the line may treat as a value distinct
    /// from `0`. Imperial has the same trap one conversion away: -18 °C is
    /// -0.4 °F.
    #[test]
    fn a_reading_just_below_zero_prints_as_zero_not_negative_zero() {
        let mut weather = Weather::fixture(5, 2);
        weather.current.temp_c = Some(-0.3);
        weather.current.feels_like_c = Some(-18.0);

        let metric = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(metric.contains("\n0°C, feels like -18°C"), "{metric}");

        let imperial = report("Berlin, Germany", &weather, Unit::Imperial, here());
        assert!(imperial.contains("\n31°F, feels like 0°F"), "{imperial}");
        assert!(!imperial.contains("-0"), "{imperial}");
    }

    /// The live reading rides the conditions line: "now" is the question, so
    /// the current AQI is the answer, not today's maximum.
    #[test]
    fn air_quality_joins_the_conditions_line_when_it_arrived() {
        let mut weather = Weather::fixture(5, 2);
        weather.air_quality = Some(AirQuality { us_aqi: 42 });

        let text = report("Berlin, Germany", &weather, Unit::Metric, here());
        let conditions = text.lines().nth(1).expect("a conditions line");

        assert!(conditions.ends_with("AQI 42 Good"), "{conditions}");
    }

    /// A missing measurement vanishes instead of printing a placeholder: a
    /// script reading the output wants absent things absent.
    #[test]
    fn missing_readings_vanish_rather_than_leaving_dashes() {
        let mut weather = Weather::fixture(5, 2);
        weather.current = Current {
            temp_c: Some(25.0),
            feels_like_c: None,
            code: None,
            wind_kph: None,
        };

        let text = report("Berlin, Germany", &weather, Unit::Metric, here());

        assert!(text.starts_with("Berlin, Germany\n25°C\n"), "{text}");
        assert!(!text.contains("--"), "{text}");
        assert!(!text.contains("feels like"), "{text}");
    }

    #[test]
    fn a_conditions_line_with_nothing_to_say_is_dropped_whole() {
        let mut weather = Weather::fixture(5, 2);
        weather.current = Current {
            temp_c: None,
            feels_like_c: None,
            code: None,
            wind_kph: None,
        };
        weather.air_quality = None;

        let text = report("Berlin, Germany", &weather, Unit::Metric, here());

        assert_eq!(
            text.lines().count(),
            2,
            "an empty line printed anyway: {text:?}"
        );
        assert!(text.starts_with("Berlin, Germany\nToday:"), "{text}");
    }

    /// Feels-like without a temperature keeps its label — a bare number would
    /// read as the temperature it is not.
    #[test]
    fn feels_like_alone_keeps_its_label() {
        let mut weather = Weather::fixture(5, 2);
        weather.current.temp_c = None;

        let text = report("Berlin, Germany", &weather, Unit::Metric, here());

        assert!(text.contains("feels like 26°C"), "{text}");
    }

    #[test]
    fn no_daily_data_means_no_outlook_line() {
        let mut weather = Weather::fixture(5, 2);
        weather.daily.clear();

        let text = report("Berlin, Germany", &weather, Unit::Metric, here());

        assert!(!text.contains("Today:"), "{text}");
    }

    /// The chance is what people plan around; the amount steps in only when
    /// the chance is missing and something is actually forecast to fall.
    #[test]
    fn a_forecast_amount_stands_in_when_the_chance_is_missing() {
        let mut weather = Weather::fixture(5, 2);
        weather.daily[2].rain_chance = None;

        let metric = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(metric.contains("rain 2.5 mm"), "{metric}");

        let imperial = report("Berlin, Germany", &weather, Unit::Imperial, here());
        assert!(imperial.contains("rain 0.10 in"), "{imperial}");

        weather.daily[2].precip_mm = Some(0.0);
        let dry = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(
            !dry.contains("rain"),
            "no chance and no accumulation is not rain: {dry}"
        );
    }

    /// Half a sun is no sun: the hours print only when both ends arrived
    /// looking like timestamps.
    #[test]
    fn sun_hours_need_both_ends_and_a_readable_stamp() {
        let mut weather = Weather::fixture(5, 2);
        weather.daily[2].sunset = None;
        let text = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(!text.contains("sun "), "{text}");

        let mut weather = Weather::fixture(5, 2);
        weather.daily[2].sunrise = Some("06:00".to_string());
        let text = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(
            !text.contains("sun "),
            "a dateless stamp was trusted: {text}"
        );
    }

    /// A city whose clocks disagree with the machine's says what time it is
    /// there, on the first line, so the report stays the same number of
    /// lines whichever city it is about.
    #[test]
    fn a_distant_city_reports_its_local_time_on_the_first_line() {
        let mut weather = Weather::fixture(5, 2);
        weather.utc_offset_secs = Some(9 * 3600);

        let text = report("Tokyo, Japan", &weather, Unit::Metric, here());

        assert_eq!(
            text.lines().next(),
            Some("Tokyo, Japan · Clear sky · local time 21:30 (UTC+09:00)")
        );
        assert_eq!(text.lines().count(), 3, "{text}");

        weather.utc_offset_secs = Some(-(7 * 3600));
        let text = report(
            "Frederick, Colorado, United States",
            &weather,
            Unit::Metric,
            here(),
        );
        assert!(
            text.starts_with(
                "Frederick, Colorado, United States · Clear sky · local time 05:30 (UTC-07:00)\n"
            ),
            "{text}"
        );
    }

    /// The same clock has nothing to say, whether the offset matches the
    /// machine's or was never known; and a nonsense offset is not believed.
    #[test]
    fn a_city_on_the_machines_clock_says_nothing_about_the_time() {
        let mut weather = Weather::fixture(5, 2);
        weather.utc_offset_secs = Some(2 * 3600);
        let text = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(!text.contains("local time"), "{text}");

        weather.utc_offset_secs = None;
        let text = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(!text.contains("local time"), "{text}");

        weather.utc_offset_secs = Some(30 * 3600);
        let text = report("Berlin, Germany", &weather, Unit::Metric, here());
        assert!(!text.contains("local time"), "{text}");
    }

    /// No sky and a far-away clock: the time still joins the place, with no
    /// dangling separator where the description would have been.
    #[test]
    fn the_local_time_joins_the_place_when_the_sky_is_unknown() {
        let mut weather = Weather::fixture(5, 2);
        weather.current.code = None;
        weather.utc_offset_secs = Some(9 * 3600);

        let text = report("Tokyo, Japan", &weather, Unit::Metric, here());

        assert_eq!(
            text.lines().next(),
            Some("Tokyo, Japan · local time 21:30 (UTC+09:00)")
        );
    }

    /// The one unconditional promise: whatever the response was missing, the
    /// report names the place it describes.
    #[test]
    fn the_place_always_prints() {
        let weather = Weather {
            current: Current {
                temp_c: None,
                feels_like_c: None,
                code: None,
                wind_kph: None,
            },
            daily: Vec::new(),
            today_index: 0,
            utc_offset_secs: None,
            hourly: Vec::new(),
            now_hour: 0,
            air_quality: None,
        };

        assert_eq!(
            report("Berlin, Germany", &weather, Unit::Metric, here()),
            "Berlin, Germany"
        );
    }
}
