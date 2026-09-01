use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct Location {
    pub name: String,
    pub admin1: Option<String>,
    pub country: Option<String>,
    pub lat: f64,
    pub lon: f64,
}

impl Location {
    pub fn label(&self) -> String {
        let mut parts = vec![self.name.clone()];
        if let Some(admin1) = &self.admin1 {
            parts.push(admin1.clone());
        }
        if let Some(country) = &self.country {
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
    /// When the reading is for, local to the location and as the endpoint
    /// spells it, e.g. "2026-08-09T17:45". The cache relocates "now" from
    /// it; nothing on screen prints it.
    pub observed: Option<String>,
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
}

impl HourlyForecast {
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
    pub air_quality: Option<AirQuality>,
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

        let day = self.daily.iter().position(|d| d.date == date);
        let today_index =
            day.unwrap_or_else(|| self.daily.iter().filter(|d| d.date.as_str() < date).count());

        let at = self
            .hourly
            .iter()
            .position(|h| h.time.get(..13) == Some(hour));
        let now_hour = at.unwrap_or_else(|| {
            self.hourly
                .iter()
                .filter(|h| h.time.as_str() < hour)
                .count()
        });

        Position {
            today_index,
            now_hour,
            exact: day.is_some() && at.is_some(),
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
                observed: None,
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
                })
                .collect(),
            now_hour: PAST_HOURS,
            air_quality: None,
        }
    }
}

/// The fixture's hourly series carries history either side of `now_hour`, as
/// the real response does — a screen that looks only forward has to prove it.
#[cfg(test)]
const PAST_HOURS: usize = 24;
#[cfg(test)]
const FORECAST_HOURS: usize = 192;

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
}
