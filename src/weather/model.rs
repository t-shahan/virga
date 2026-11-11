use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The shape of every timestamp in a forecast response, local to the place.
pub(crate) const STAMP: &str = "%Y-%m-%dT%H:%M";
const QUARTER: TimeDelta = TimeDelta::minutes(15);
/// Two hours. The request asks for exactly this many, and the cap is here
/// because the answer's length is the server's word.
pub const NOWCAST_QUARTERS: usize = 8;

pub struct Location {
    pub name: String,
    pub admin1: Option<String>,
    pub country: Option<String>,
    pub lat: f64,
    pub lon: f64,
    /// The IANA zone the geocoder files the place under, e.g.
    /// "America/Denver". Not persisted: what the state file and the cache
    /// keep is `ActiveLocation`, and the offset the forecast response
    /// carries is what tells the time there.
    pub timezone: Option<String>,
    /// ISO 3166-1 alpha-2, e.g. "US". Two rows that collide already share
    /// the country the label names, so the code is read only where the
    /// geocoder sent no country name and it is the one thing left that says
    /// which country the place is in.
    pub country_code: Option<String>,
    /// From the geocoder, where it knows. Orders the search list so the city
    /// people mean comes first among the places that share its name.
    pub population: Option<u64>,
}

impl Location {
    pub fn label(&self) -> String {
        let mut parts = vec![self.name.clone()];
        if let Some(admin1) = &self.admin1 {
            parts.push(admin1.clone());
        }
        if let Some(country) = self.country.as_ref().or(self.country_code.as_ref()) {
            parts.push(country.clone());
        }
        parts.join(", ")
    }
}

/// Each reading is independently optional — the API can report some current
/// conditions and not others, so a missing wind speed should not blank the
/// temperature.
#[derive(Serialize, Deserialize)]
pub struct Current {
    pub temp_c: Option<f64>,
    pub feels_like_c: Option<f64>,
    pub code: Option<u8>,
    pub wind_kph: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct DailyForecast {
    pub date: String,
    pub high_c: f64,
    pub low_c: f64,
    pub code: u8,
    /// Supplementary readings. Unlike the four above, a missing value here
    /// leaves a blank cell rather than dropping the whole day.
    pub rain_chance: Option<u8>,
    pub wind_kph: Option<f64>,
    pub uv_index: Option<f64>,
    /// Highest US AQI recorded for the day, where the endpoint covers it.
    pub aqi: Option<u16>,
    /// ISO timestamps, e.g. "2026-08-09T06:17".
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub feels_max_c: Option<f64>,
    pub feels_min_c: Option<f64>,
    pub precip_mm: Option<f64>,
    pub precip_hours: Option<f64>,
    pub gust_kph: Option<f64>,
    pub wind_dir_deg: Option<f64>,
    pub daylight_secs: Option<f64>,
}

#[derive(Clone, Serialize, Deserialize)]
/// One hour of the forecast. Only the timestamp is guaranteed; every reading
/// can come back null and degrades to a dash rather than dropping the hour.
pub struct HourlyForecast {
    /// Local to the location, e.g. "2026-08-10T16:00".
    pub time: String,
    /// Total precipitation, snow counted as its melted equivalent.
    pub precip_mm: Option<f64>,
    /// Snow depth, which is a different measure of the same hour — 1 mm of
    /// precipitation is roughly 0.7 cm of snow, not 0.1.
    pub snow_cm: Option<f64>,
    pub chance: Option<u8>,
    pub code: Option<u8>,
    pub temp_c: Option<f64>,
    pub feels_like_c: Option<f64>,
    pub humidity_pct: Option<u8>,
    pub wind_kph: Option<f64>,
    pub gust_kph: Option<f64>,
    pub wind_dir_deg: Option<f64>,
    /// How far the ensemble's runs disagree about this hour's temperature.
    /// Defaulted so a cache written before the field existed still parses.
    #[serde(default)]
    pub spread: Option<TempSpread>,
}

/// The middle 80% of an ensemble's runs for one hour, as distances from
/// their own median, in Celsius degrees and never negative.
///
/// Distances rather than temperatures because the ensemble is a coarser
/// model than the one the forecast comes from, and over hills or a coast
/// its runs can sit degrees away from the forecast as a body. How much they
/// disagree with each other survives that; where they sit does not. The
/// band is therefore drawn around the forecast's own temperature.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TempSpread {
    pub below_c: f64,
    pub above_c: f64,
}

impl HourlyForecast {
    /// The range most runs put this hour's temperature in, around the
    /// forecast's own figure. `None` without both a temperature and a spread.
    pub fn likely_range_c(&self) -> Option<(f64, f64)> {
        let (temp, spread) = (self.temp_c?, self.spread?);
        Some((temp - spread.below_c, temp + spread.above_c))
    }

    /// Whether anything is forecast to fall. The trigger is the amount rather
    /// than the chance: a 20% hour with no forecast accumulation is not rain,
    /// which is the confusion the daily pane already had to fix once.
    pub fn is_wet(&self) -> bool {
        self.precip_mm.is_some_and(|mm| mm > 0.0)
    }

    /// Snow is the story when there is any, since a centimetre of snow and a
    /// millimetre of rain are very different news.
    pub fn is_snow(&self) -> bool {
        self.snow_cm.is_some_and(|cm| cm > 0.0)
    }
}

/// A quarter hour of the next two, from the provider's 15-minute series.
/// Native model data over North America and central Europe and interpolated
/// from the hourly series elsewhere; the response does not say which, so
/// nothing built on this may claim more precision than "about".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QuarterHour {
    /// When the quarter starts, local to the location, like every hour's.
    pub time: String,
    pub precip_mm: Option<f64>,
    pub snow_cm: Option<f64>,
}

#[derive(Serialize, Deserialize)]
pub struct Weather {
    pub current: Current,
    /// Past days, today, then the forecast — in chronological order.
    pub daily: Vec<DailyForecast>,
    /// Index of today within `daily`.
    pub today_index: usize,
    /// The hourly series, which like `daily` runs from the past through the
    /// forecast horizon.
    pub hourly: Vec<HourlyForecast>,
    /// Index of the current hour within `hourly`.
    pub now_hour: usize,
    /// The location's UTC offset when this was fetched, in seconds east,
    /// from the response's `utc_offset_seconds`. The cache derives the
    /// location's current time from it — never from the user's clock alone,
    /// and never from the reading's stamp, which model data can hold up to
    /// a quarter hour behind the fetch. Defaulted so a cache written before
    /// the field existed parses, and is then refused for lacking it.
    #[serde(default)]
    pub utc_offset_secs: Option<i32>,
    pub air_quality: Option<AirQuality>,
    /// The provider's next two hours in quarter-hour steps, as fetched.
    /// Never read directly: it describes the minutes after the fetch, and
    /// `nowcast_at` is what turns it into the minutes after now.
    /// Defaulted so a cache written before the field existed still parses.
    #[serde(default)]
    pub nowcast: Vec<QuarterHour>,
}

impl Weather {
    /// The hourly forecast from this hour onward. The request carries two weeks
    /// of history for the daily chart's benefit; the hourly screen looks
    /// only forward, so it slices here rather than every caller doing its own
    /// arithmetic on `now_hour`.
    pub fn forecast_hours(&self) -> &[HourlyForecast] {
        self.hourly.get(self.now_hour..).unwrap_or_default()
    }

    /// Where a location-local time such as "2026-08-10T19:15" falls in the
    /// series.
    ///
    /// The date is matched whole and the hour is matched to the hour, so
    /// 19:15 lands on the 19:00 entry. An absent entry falls back to the
    /// count of entries before it, the boundary between history and
    /// forecast, which is the honest answer when the entry itself was
    /// dropped: a positional index would otherwise present the whole
    /// history as forecast.
    pub fn position_of(&self, stamp: &str) -> Position {
        let date = stamp.get(..10).unwrap_or(stamp);
        let hour = stamp.get(..13).unwrap_or(stamp);

        let (today_index, day_found) = index_of(self.daily.iter().map(|d| d.date.as_str()), date);
        let (now_hour, hour_found) = index_of(
            self.hourly
                .iter()
                .map(|h| h.time.get(..13).unwrap_or(&h.time)),
            hour,
        );

        Position {
            today_index,
            now_hour,
            exact: day_found && hour_found,
        }
    }

    /// Point today and now at `stamp`. A forecast read back from disk was
    /// parsed at some earlier hour, and its positions say so until moved.
    /// Refused, leaving the positions alone, when the stamp is outside the
    /// series: a boundary fallback is right for a dropped row inside a fresh
    /// response and wrong for a forecast that has simply run out.
    pub fn relocate(&mut self, stamp: &str) -> bool {
        let position = self.position_of(stamp);
        if !position.exact {
            return false;
        }
        self.today_index = position.today_index;
        self.now_hour = position.now_hour;
        true
    }

    /// The quarter hour in progress at `now` and the ones after it, in
    /// step and with nothing missing between them, so a position in the
    /// result is a number of minutes from `now`.
    ///
    /// Asked for at every read and never stored, because the answer ages:
    /// a sentence counted at the fetch still said "rain in about 30 min"
    /// two hours into an open session. Empty without the response's UTC
    /// offset, which is the only thing that says what time it is there.
    pub fn nowcast_at(&self, now: DateTime<Utc>) -> Vec<QuarterHour> {
        let Some(offset) = self.utc_offset_secs.and_then(FixedOffset::east_opt) else {
            return Vec::new();
        };
        let stamp = now.with_timezone(&offset).format(STAMP).to_string();
        aligned_quarters(&self.nowcast, &stamp)
    }
}

pub(crate) fn parse_stamp(stamp: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(stamp, STAMP).ok()
}

/// The part of `quarters` that opens on the quarter hour holding `stamp`
/// and runs on in 15-minute steps. Quarters that have ended are dropped,
/// and a series that opens later than `stamp`, skips a step, or carries a
/// time that will not parse ends where that happens: "rain in about 30 min"
/// counted from the wrong quarter is worse than no sentence.
fn aligned_quarters(quarters: &[QuarterHour], stamp: &str) -> Vec<QuarterHour> {
    let Some(now) = parse_stamp(stamp) else {
        return Vec::new();
    };
    let mut expected = None;
    quarters
        .iter()
        .skip_while(|quarter| parse_stamp(&quarter.time).is_none_or(|start| start + QUARTER <= now))
        .take_while(|quarter| {
            let start = parse_stamp(&quarter.time);
            let in_step = match expected {
                None => start.is_some_and(|start| start <= now),
                Some(_) => start == expected,
            };
            expected = start.map(|start| start + QUARTER);
            in_step
        })
        .take(NOWCAST_QUARTERS)
        .cloned()
        .collect()
}

/// Where `key` sits in an ordered series, and whether it was actually there.
/// A key the series lacks lands on the count of keys before it, which is the
/// boundary between history and forecast; the flag lets `relocate` tell that
/// fallback apart from a hit.
fn index_of<'a>(keys: impl Iterator<Item = &'a str> + Clone, key: &str) -> (usize, bool) {
    match keys.clone().position(|k| k == key) {
        Some(index) => (index, true),
        None => (keys.filter(|k| *k < key).count(), false),
    }
}

/// Where a moment falls in a forecast's series. See `Weather::position_of`.
pub struct Position {
    pub today_index: usize,
    pub now_hour: usize,
    /// Both the date and the hour were found, rather than either falling
    /// back to a boundary.
    pub exact: bool,
}

/// Everything one air-quality request yields: the live reading, plus a maximum
/// for each date it covers. The endpoint has no daily aggregation, so the
/// per-day figures are derived from its hourly series.
#[derive(Default)]
pub struct AirQualityReport {
    pub current: Option<AirQuality>,
    pub daily_max: HashMap<String, u16>,
}

/// Each hour an ensemble request covered, by its timestamp, which is the
/// forecast's own because both ask for the location's local time.
#[derive(Default)]
pub struct SpreadReport {
    pub by_hour: HashMap<String, TempSpread>,
}

#[derive(Serialize, Deserialize)]
pub struct AirQuality {
    pub us_aqi: u16,
}

#[cfg(test)]
impl Weather {
    /// Deterministic fixture: `days` days whose highs climb by one degree from
    /// 20C, with today at `today_index`.
    pub fn fixture(days: usize, today_index: usize) -> Self {
        Self {
            current: Current {
                temp_c: Some(25.0),
                feels_like_c: Some(26.0),
                code: Some(0),
                wind_kph: Some(10.0),
            },
            daily: (0..days)
                .map(|i| DailyForecast {
                    date: format!("2026-08-{:02}", i + 1),
                    high_c: 20.0 + i as f64,
                    low_c: 10.0 + i as f64,
                    code: 0,
                    rain_chance: Some(10),
                    wind_kph: Some(12.0),
                    uv_index: Some(6.0),
                    aqi: Some(42),
                    sunrise: Some(format!("2026-08-{:02}T06:00", i + 1)),
                    sunset: Some(format!("2026-08-{:02}T20:00", i + 1)),
                    feels_max_c: Some(22.0 + i as f64),
                    feels_min_c: Some(12.0 + i as f64),
                    precip_mm: Some(2.54),
                    precip_hours: Some(3.0),
                    gust_kph: Some(30.0),
                    wind_dir_deg: Some(315.0),
                    daylight_secs: Some(49_320.0),
                })
                .collect(),
            today_index,
            hourly: (0..PAST_HOURS + FORECAST_HOURS)
                .map(|i| HourlyForecast {
                    time: hour_stamp(i),
                    // Mostly dry, wet every seventeenth hour — the real series
                    // is far emptier than a naive fixture would suggest.
                    precip_mm: Some(if i % 17 == 0 { 0.4 } else { 0.0 }),
                    snow_cm: Some(0.0),
                    chance: Some(((i * 7) % 101) as u8),
                    code: Some(if i % 17 == 0 { 61 } else { 0 }),
                    temp_c: Some(15.0 + (i % 12) as f64),
                    feels_like_c: Some(14.0 + (i % 12) as f64),
                    humidity_pct: Some(55 + (i % 20) as u8),
                    wind_kph: Some(8.0 + (i % 10) as f64),
                    gust_kph: Some(15.0 + (i % 12) as f64),
                    wind_dir_deg: Some(((i * 45) % 360) as f64),
                    spread: None,
                })
                .collect(),
            now_hour: PAST_HOURS,
            utc_offset_secs: None,
            air_quality: None,
            nowcast: Vec::new(),
        }
    }

    /// The fixture with a nowcast opening at its current hour, one quarter
    /// per entry of `precip_mm`, at a place on UTC so `fixture_time` is
    /// both its clock and the world's.
    pub fn with_nowcast(mut self, precip_mm: &[f64]) -> Self {
        self.utc_offset_secs.get_or_insert(0);
        let start = parse_stamp(&self.hourly[self.now_hour].time).expect("a fixture stamp");
        self.nowcast = precip_mm
            .iter()
            .zip(0..)
            .map(|(&mm, i)| QuarterHour {
                time: (start + QUARTER * i).format(STAMP).to_string(),
                precip_mm: Some(mm),
                snow_cm: Some(0.0),
            })
            .collect();
        self
    }
}

/// The fixture's hourly series carries history either side of `now_hour`, as
/// the real response does — a screen that looks only forward has to prove it.
#[cfg(test)]
const PAST_HOURS: usize = 24;
#[cfg(test)]
const FORECAST_HOURS: usize = 192;

/// A UTC instant from a stamp in the fixture's own format.
#[cfg(test)]
pub fn fixture_time(stamp: &str) -> DateTime<Utc> {
    parse_stamp(stamp).expect("a fixture stamp").and_utc()
}

#[cfg(test)]
fn hour_stamp(i: usize) -> String {
    format!("2026-08-{:02}T{:02}:00", 1 + i / 24, i % 24)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture opens at 2026-08-01T00:00 with an hour per entry, so hour
    /// `n` of the series is day `n / 24`.
    #[test]
    fn relocate_moves_now_and_today_across_an_hour_and_a_day() {
        let mut weather = Weather::fixture(9, 0);

        assert!(weather.relocate("2026-08-02T03:15"));
        assert_eq!(weather.now_hour, 27, "19:15 style stamps land on the hour");
        assert_eq!(weather.today_index, 1);

        assert!(weather.relocate("2026-08-03T01:00"));
        assert_eq!(weather.now_hour, 49);
        assert_eq!(weather.today_index, 2);
    }

    /// A forecast that has run out must say so rather than pin "now" to its
    /// last hour: the boundary fallback exists for a dropped row inside a
    /// fresh response, and a fresh response is not what a stale cache is.
    #[test]
    fn relocate_refuses_a_stamp_outside_the_series() {
        let mut weather = Weather::fixture(9, 0);
        let before = (weather.now_hour, weather.today_index);

        assert!(!weather.relocate("2026-09-01T00:00"), "past the horizon");
        assert!(!weather.relocate("2026-07-31T23:00"), "before the history");
        assert_eq!((weather.now_hour, weather.today_index), before);
    }

    /// The daily series can be shorter than the hourly one; a date the days
    /// do not cover is outside the series even where the hours cover it.
    #[test]
    fn relocate_needs_both_the_day_and_the_hour() {
        let mut weather = Weather::fixture(2, 0);
        assert!(!weather.relocate("2026-08-05T00:00"));
    }

    fn starts(quarters: &[QuarterHour]) -> Vec<&str> {
        quarters.iter().map(|q| &q.time[11..]).collect()
    }

    /// The fixture's now is 2026-08-02T00:00, so its quarters open there.
    #[test]
    fn the_nowcast_drops_the_quarters_that_have_ended() {
        let weather = Weather::fixture(9, 1).with_nowcast(&[0.0; 8]);

        let running = weather.nowcast_at(fixture_time("2026-08-02T00:14"));
        assert_eq!(running.len(), 8, "the first quarter is still running");

        let later = weather.nowcast_at(fixture_time("2026-08-02T00:50"));
        assert_eq!(
            starts(&later),
            ["00:45", "01:00", "01:15", "01:30", "01:45"]
        );

        let over = weather.nowcast_at(fixture_time("2026-08-02T02:00"));
        assert!(over.is_empty(), "the window has passed");
        assert_eq!(weather.nowcast.len(), 8, "reading it changes nothing");
    }

    /// A position in the series is read as minutes from now, which holds
    /// only while the series opens on the running quarter and never skips.
    #[test]
    fn a_nowcast_that_cannot_be_lined_up_with_the_clock_is_cut_short() {
        let fresh = || Weather::fixture(9, 1).with_nowcast(&[0.0; 8]);
        let midnight = fixture_time("2026-08-02T00:00");

        let ahead = fresh().nowcast_at(fixture_time("2026-08-01T23:00"));
        assert!(ahead.is_empty(), "opens in the future");

        let mut gapped = fresh();
        gapped.nowcast.remove(3);
        assert_eq!(
            starts(&gapped.nowcast_at(midnight)),
            ["00:00", "00:15", "00:30"],
            "stops at the gap"
        );

        let mut garbled = fresh();
        garbled.nowcast[2].time = "soon".to_string();
        assert_eq!(starts(&garbled.nowcast_at(midnight)), ["00:00", "00:15"]);

        assert!(aligned_quarters(&fresh().nowcast, "not a time").is_empty());
    }

    /// The clock that counts is the place's, not the machine's or UTC's:
    /// 23:35 UTC is 01:05 at a place an hour and a half ahead.
    #[test]
    fn the_nowcast_is_read_on_the_places_own_clock() {
        let mut weather = Weather::fixture(9, 1).with_nowcast(&[0.0; 8]);
        weather.utc_offset_secs = Some(5400);

        let there = weather.nowcast_at(fixture_time("2026-08-01T23:35"));
        assert_eq!(starts(&there), ["01:00", "01:15", "01:30", "01:45"]);

        weather.utc_offset_secs = None;
        assert!(
            weather
                .nowcast_at(fixture_time("2026-08-02T00:00"))
                .is_empty(),
            "no offset, no way to say what time it is there"
        );
    }

    #[test]
    fn an_oversized_nowcast_is_capped() {
        let weather = Weather::fixture(9, 1).with_nowcast(&[0.0; 40]);
        let now = fixture_time("2026-08-02T00:00");
        assert_eq!(weather.nowcast_at(now).len(), NOWCAST_QUARTERS);
    }

    /// A cache written before the ensemble was asked for has hours with no
    /// `spread` key, and they still have to read.
    #[test]
    fn an_hour_cached_without_a_spread_still_parses() {
        let mut json = serde_json::to_value(&Weather::fixture(2, 0).hourly[0]).unwrap();
        json.as_object_mut()
            .unwrap()
            .remove("spread")
            .expect("the field is written");

        let hour: HourlyForecast = serde_json::from_value(json).expect("an older hour");
        assert_eq!(hour.spread, None);
        assert_eq!(hour.likely_range_c(), None);
    }

    #[test]
    fn the_likely_range_sits_around_the_forecasts_own_temperature() {
        let mut hour = Weather::fixture(2, 0).hourly[0].clone();
        hour.temp_c = Some(10.0);
        hour.spread = Some(TempSpread {
            below_c: 1.5,
            above_c: 4.0,
        });
        assert_eq!(hour.likely_range_c(), Some((8.5, 14.0)));

        hour.temp_c = None;
        assert_eq!(hour.likely_range_c(), None, "nothing to centre it on");
    }

    fn place(country: Option<&str>, country_code: Option<&str>) -> Location {
        Location {
            name: "Frederick".to_string(),
            admin1: Some("Maryland".to_string()),
            country: country.map(str::to_string),
            lat: 39.41,
            lon: -77.41,
            timezone: None,
            country_code: country_code.map(str::to_string),
            population: None,
        }
    }

    /// The code stands in for the country only when there is no name for
    /// it; beside a name it would repeat what the label already says.
    #[test]
    fn the_label_falls_back_to_the_country_code_when_the_name_is_missing() {
        assert_eq!(
            place(Some("United States"), Some("US")).label(),
            "Frederick, Maryland, United States"
        );
        assert_eq!(place(None, Some("US")).label(), "Frederick, Maryland, US");
        assert_eq!(place(None, None).label(), "Frederick, Maryland");
    }
}
