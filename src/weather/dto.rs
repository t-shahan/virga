use crate::weather::model::{
    AirQuality, AirQualityReport, Current, DailyForecast, HourlyForecast, Location, Weather,
};
use chrono::Local;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct GeocodeResultDto {
    #[serde(default)]
    pub name: String,
    pub admin1: Option<String>,
    pub country: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct GeocodeDto {
    #[serde(default)]
    pub results: Vec<GeocodeResultDto>,
}

impl GeocodeResultDto {
    /// A result without coordinates cannot be fetched for, so it is dropped
    /// rather than surfaced as an unselectable row.
    pub fn into_location(self) -> Option<Location> {
        Some(Location {
            name: self.name,
            admin1: self.admin1,
            country: self.country,
            lat: self.latitude?,
            lon: self.longitude?,
        })
    }
}

/// What an IP geolocation service says about the caller.
///
/// Every field is optional and the conversion does the rejecting, the same way
/// `GeocodeResultDto` does: a body that parses is not the same thing as a body
/// that names a place.
#[derive(Debug, Deserialize)]
pub struct GeoIpDto {
    pub city: Option<String>,
    pub region: Option<String>,
    pub country_name: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    /// The provider's in-band failure: rate limiting and reserved addresses
    /// come back as a *200* carrying this rather than as an error status. A
    /// body with it set is not a place, whatever else it carries.
    #[serde(default)]
    pub error: bool,
}

impl GeoIpDto {
    pub fn into_location(self) -> Option<Location> {
        if self.error {
            return None;
        }

        let (lat, lon) = (self.latitude?, self.longitude?);
        if !lat.is_finite() || !lon.is_finite() {
            return None;
        }
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return None;
        }
        // Null Island is the shape "unknown" takes when a provider answers
        // anyway. Nobody launches the app in the Gulf of Guinea.
        if lat == 0.0 && lon == 0.0 {
            return None;
        }

        // The border shows this label, so an empty one would be a blank heading
        // over a real forecast. City, region and country are each optional; all
        // three absent is a reading, not a location.
        let (name, admin1, country) = match (self.city, self.region) {
            (Some(city), region) => (city, region, self.country_name),
            (None, Some(region)) => (region, None, self.country_name),
            // The country is the whole name here, so repeating it as the
            // country would label the city "Iceland, Iceland".
            (None, None) => (self.country_name?, None, None),
        };

        Some(Location {
            name,
            admin1,
            country,
            lat,
            lon,
        })
    }
}

/// Same reasoning as `DailyDto`: any individual measurement can come back null,
/// and one missing value must not cost us the entire response.
#[derive(Debug, Deserialize)]
pub struct CurrentDto {
    pub time: Option<String>,
    pub temperature_2m: Option<f64>,
    pub apparent_temperature: Option<f64>,
    pub weather_code: Option<u8>,
    pub wind_speed_10m: Option<f64>,
}

/// Every measurement is optional: Open-Meteo returns `null` for days a model
/// has no data for. Typing these as bare `f64` makes a single null fail the
/// whole response rather than one day.
#[derive(Debug, Deserialize)]
pub struct DailyDto {
    pub time: Vec<String>,
    pub weather_code: Vec<Option<u8>>,
    pub temperature_2m_max: Vec<Option<f64>>,
    pub temperature_2m_min: Vec<Option<f64>>,
    #[serde(default)]
    pub precipitation_probability_max: Vec<Option<u8>>,
    #[serde(default)]
    pub wind_speed_10m_max: Vec<Option<f64>>,
    #[serde(default)]
    pub uv_index_max: Vec<Option<f64>>,
    #[serde(default)]
    pub sunrise: Vec<Option<String>>,
    #[serde(default)]
    pub sunset: Vec<Option<String>>,
    #[serde(default)]
    pub apparent_temperature_max: Vec<Option<f64>>,
    #[serde(default)]
    pub apparent_temperature_min: Vec<Option<f64>>,
    #[serde(default)]
    pub precipitation_sum: Vec<Option<f64>>,
    #[serde(default)]
    pub precipitation_hours: Vec<Option<f64>>,
    #[serde(default)]
    pub wind_gusts_10m_max: Vec<Option<f64>>,
    #[serde(default)]
    pub wind_direction_10m_dominant: Vec<Option<f64>>,
    #[serde(default)]
    pub daylight_duration: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
pub struct ForecastDto {
    pub current: CurrentDto,
    pub daily: DailyDto,
    /// Optional so a request that never asked for an hourly block — or a
    /// fixture recorded before it did — still parses.
    pub hourly: Option<HourlyDto>,
}

/// Same rules as `DailyDto`: every measurement optional, so one null hour
/// cannot cost the whole series.
#[derive(Debug, Deserialize)]
pub struct HourlyDto {
    #[serde(default)]
    pub time: Vec<String>,
    #[serde(default)]
    pub precipitation: Vec<Option<f64>>,
    #[serde(default)]
    pub precipitation_probability: Vec<Option<u8>>,
    /// Centimetres of snow, where `precipitation` counts its melted equivalent
    /// in millimetres. Two different measures of the same hour.
    #[serde(default)]
    pub snowfall: Vec<Option<f64>>,
    #[serde(default)]
    pub weather_code: Vec<Option<u8>>,
    #[serde(default)]
    pub temperature_2m: Vec<Option<f64>>,
    #[serde(default)]
    pub apparent_temperature: Vec<Option<f64>>,
    #[serde(default)]
    pub relative_humidity_2m: Vec<Option<u8>>,
    #[serde(default)]
    pub wind_speed_10m: Vec<Option<f64>>,
    #[serde(default)]
    pub wind_gusts_10m: Vec<Option<f64>>,
    #[serde(default)]
    pub wind_direction_10m: Vec<Option<f64>>,
}

/// `values[i]`, collapsing "index out of range" and "value was null" into the
/// same `None` — the caller cares about neither distinction.
fn at<T: Copy>(values: &[Option<T>], i: usize) -> Option<T> {
    values.get(i).copied().flatten()
}

/// `at` for values that are not `Copy`.
fn at_owned(values: &[Option<String>], i: usize) -> Option<String> {
    values.get(i)?.clone()
}

impl From<ForecastDto> for Weather {
    fn from(dto: ForecastDto) -> Self {
        // `current.time` is local to the forecast location, so its date identifies
        // today's entry without assuming how many past days were requested.
        // Falls back to the system date if the API omits its own clock. That is
        // wrong for a city in another timezone, but only by a day at the edges,
        // and it beats defaulting the index to zero — which would present the
        // whole history as forecast.
        let today = dto
            .current
            .time
            .as_deref()
            .and_then(|stamp| stamp.get(..10))
            .map(str::to_string)
            .unwrap_or_else(|| Local::now().date_naive().to_string());

        // Seven parallel arrays would make a zip chain unreadable — the pattern
        // becomes ((((((a, b), c), d), e), f), g) — so index instead. Costs a
        // clone per date string, which is nothing against the legibility.
        //
        // `?` on the core four drops a day that is missing them; the
        // supplementary readings stay Option, so a null UV only blanks a cell.
        let day = &dto.daily;
        let daily: Vec<DailyForecast> = (0..day.time.len())
            .filter_map(|i| {
                Some(DailyForecast {
                    date: day.time.get(i)?.clone(),
                    high_c: at(&day.temperature_2m_max, i)?,
                    low_c: at(&day.temperature_2m_min, i)?,
                    code: at(&day.weather_code, i)?,
                    rain_chance: at(&day.precipitation_probability_max, i),
                    wind_kph: at(&day.wind_speed_10m_max, i),
                    uv_index: at(&day.uv_index_max, i),
                    aqi: None,
                    sunrise: at_owned(&day.sunrise, i),
                    sunset: at_owned(&day.sunset, i),
                    feels_max_c: at(&day.apparent_temperature_max, i),
                    feels_min_c: at(&day.apparent_temperature_min, i),
                    precip_mm: at(&day.precipitation_sum, i),
                    precip_hours: at(&day.precipitation_hours, i),
                    gust_kph: at(&day.wind_gusts_10m_max, i),
                    wind_dir_deg: at(&day.wind_direction_10m_dominant, i),
                    daylight_secs: at(&day.daylight_duration, i),
                })
            })
            .collect();

        // Located after filtering, so a dropped row can't shift the index. If
        // today's own row was dropped, fall back to the count of days before it.
        let today_index = daily
            .iter()
            .position(|day| day.date == today)
            .unwrap_or_else(|| daily.iter().filter(|day| day.date < today).count());

        // Same index-by-position shape as `daily`, for the same reason: many
        // parallel arrays make a zip chain unreadable.
        let hourly: Vec<HourlyForecast> = dto.hourly.map_or_else(Vec::new, |hour| {
            (0..hour.time.len())
                .filter_map(|i| {
                    Some(HourlyForecast {
                        time: hour.time.get(i)?.clone(),
                        precip_mm: at(&hour.precipitation, i),
                        snow_cm: at(&hour.snowfall, i),
                        chance: at(&hour.precipitation_probability, i),
                        code: at(&hour.weather_code, i),
                        temp_c: at(&hour.temperature_2m, i),
                        feels_like_c: at(&hour.apparent_temperature, i),
                        humidity_pct: at(&hour.relative_humidity_2m, i),
                        wind_kph: at(&hour.wind_speed_10m, i),
                        gust_kph: at(&hour.wind_gusts_10m, i),
                        wind_dir_deg: at(&hour.wind_direction_10m, i),
                    })
                })
                .collect()
        });

        // The hourly series runs 14 days back as well as forward, so getting
        // this wrong points the whole screen at history. Truncating to the hour
        // matches "2026-08-10T19:15" against "2026-08-10T19:00"; the same
        // system-clock fallback as `today` applies if the API omits its clock.
        let this_hour = dto
            .current
            .time
            .as_deref()
            .and_then(|stamp| stamp.get(..13))
            .map_or_else(
                || Local::now().format("%Y-%m-%dT%H").to_string(),
                str::to_string,
            );

        // Located after filtering, exactly as `today_index` is, so a dropped
        // hour cannot shift it.
        let now_hour = hourly
            .iter()
            .position(|h| h.time.get(..13) == Some(this_hour.as_str()))
            .unwrap_or_else(|| {
                hourly
                    .iter()
                    .filter(|h| h.time.as_str() < this_hour.as_str())
                    .count()
            });

        Self {
            hourly,
            now_hour,
            current: Current {
                temp_c: dto.current.temperature_2m,
                feels_like_c: dto.current.apparent_temperature,
                code: dto.current.weather_code,
                wind_kph: dto.current.wind_speed_10m,
            },
            daily,
            today_index,
            air_quality: None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AqiDto {
    pub current: Option<AqiCurrentDto>,
    pub hourly: Option<AqiHourlyDto>,
}

#[derive(Debug, Deserialize)]
pub struct AqiHourlyDto {
    #[serde(default)]
    pub time: Vec<String>,
    #[serde(default)]
    pub us_aqi: Vec<Option<u16>>,
}

impl From<AqiDto> for AirQualityReport {
    fn from(dto: AqiDto) -> Self {
        let current = dto
            .current
            .and_then(|current| current.us_aqi)
            .map(|us_aqi| AirQuality { us_aqi });

        // The endpoint offers no daily aggregation, so collapse the hourly
        // series to a maximum per date. The max is the figure worth showing:
        // it is the worst the air got, not an average that hides a bad hour.
        let mut daily_max: HashMap<String, u16> = HashMap::new();
        if let Some(hourly) = dto.hourly {
            for (stamp, value) in hourly.time.into_iter().zip(hourly.us_aqi) {
                let (Some(value), Some(date)) = (value, stamp.get(..10)) else {
                    continue;
                };
                daily_max
                    .entry(date.to_string())
                    .and_modify(|worst| *worst = (*worst).max(value))
                    .or_insert(value);
            }
        }

        Self { current, daily_max }
    }
}

#[derive(Debug, Deserialize)]
pub struct AqiCurrentDto {
    pub us_aqi: Option<u16>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real icon_seamless response; its final day has a null temperature_2m_max.
    /// Before daily measurements were Option, this failed to deserialize at all
    /// and took the whole forecast down with it.
    #[test]
    fn drops_days_missing_measurements_instead_of_failing() {
        let json = include_str!("../../tests/fixtures/forecast_nulls.json");
        let dto: ForecastDto = serde_json::from_str(json).expect("nulls should parse");

        let raw_days = dto.daily.time.len();
        assert_eq!(raw_days, 8);

        let weather: Weather = dto.into();

        assert_eq!(weather.daily.len(), 7, "the null day should be dropped");
        assert!(
            weather.daily.iter().all(|d| d.date != "2026-08-16"),
            "the dropped day should be the one with a null max"
        );
    }

    /// Every current reading null and `timezone` absent entirely. The forecast
    /// should still come through — a blank current pane beats no weather at all.
    #[test]
    fn tolerates_null_current_readings() {
        let json = r#"{
            "current": {
                "time": null, "temperature_2m": null, "apparent_temperature": null,
                "weather_code": null, "wind_speed_10m": null
            },
            "daily": {
                "time": ["2026-08-09"], "weather_code": [3],
                "temperature_2m_max": [30.0], "temperature_2m_min": [20.0]
            }
        }"#;

        let dto: ForecastDto = serde_json::from_str(json).expect("all-null current should parse");
        let weather: Weather = dto.into();

        assert!(weather.current.temp_c.is_none());
        assert!(weather.current.feels_like_c.is_none());
        assert!(weather.current.code.is_none());
        assert!(weather.current.wind_kph.is_none());
        assert_eq!(weather.daily.len(), 1, "the forecast still comes through");
    }

    #[test]
    fn drops_geocode_results_without_coordinates() {
        let json = r#"{"results": [
            {"name": "Nowhere"},
            {"name": "Frederick", "latitude": 39.41, "longitude": -77.41}
        ]}"#;

        let dto: GeocodeDto = serde_json::from_str(json).expect("should parse");
        let locations: Vec<_> = dto
            .results
            .into_iter()
            .filter_map(GeocodeResultDto::into_location)
            .collect();

        assert_eq!(locations.len(), 1, "the coordinate-less result is dropped");
        assert_eq!(locations[0].name, "Frederick");
    }

    /// today_index is located by matching current.time against the daily dates,
    /// so it stays right whatever past_days was requested — and it is computed
    /// after filtering, so a dropped row cannot shift it.
    #[test]
    fn today_index_points_at_the_current_date() {
        let json = r#"{
            "current": {"time": "2026-08-09T14:30", "temperature_2m": 20.0,
                        "apparent_temperature": 20.0, "weather_code": 0, "wind_speed_10m": 5.0},
            "daily": {
                "time": ["2026-08-07", "2026-08-08", "2026-08-09", "2026-08-10"],
                "weather_code": [0, 0, 0, 0],
                "temperature_2m_max": [30.0, 31.0, 32.0, 33.0],
                "temperature_2m_min": [20.0, 21.0, 22.0, 23.0]
            }
        }"#;

        let weather: Weather = serde_json::from_str::<ForecastDto>(json).unwrap().into();

        assert_eq!(weather.today_index, 2);
        assert_eq!(weather.daily[weather.today_index].date, "2026-08-09");
    }

    #[test]
    fn today_index_survives_an_earlier_day_being_dropped() {
        let json = r#"{
            "current": {"time": "2026-08-09T14:30", "temperature_2m": 20.0,
                        "apparent_temperature": 20.0, "weather_code": 0, "wind_speed_10m": 5.0},
            "daily": {
                "time": ["2026-08-07", "2026-08-08", "2026-08-09", "2026-08-10"],
                "weather_code": [0, 0, 0, 0],
                "temperature_2m_max": [30.0, null, 32.0, 33.0],
                "temperature_2m_min": [20.0, 21.0, 22.0, 23.0]
            }
        }"#;

        let weather: Weather = serde_json::from_str::<ForecastDto>(json).unwrap().into();

        assert_eq!(weather.daily.len(), 3, "the null day is dropped");
        assert_eq!(
            weather.daily[weather.today_index].date, "2026-08-09",
            "the index followed the day, not its old position"
        );
    }

    /// If today's own row is the one dropped, the index must still land on the
    /// boundary rather than at zero, which would present history as forecast.
    #[test]
    fn today_index_falls_back_to_the_boundary() {
        let json = r#"{
            "current": {"time": "2026-08-09T14:30", "temperature_2m": 20.0,
                        "apparent_temperature": 20.0, "weather_code": 0, "wind_speed_10m": 5.0},
            "daily": {
                "time": ["2026-08-07", "2026-08-08", "2026-08-09", "2026-08-10"],
                "weather_code": [0, 0, 0, 0],
                "temperature_2m_max": [30.0, 31.0, null, 33.0],
                "temperature_2m_min": [20.0, 21.0, 22.0, 23.0]
            }
        }"#;

        let weather: Weather = serde_json::from_str::<ForecastDto>(json).unwrap().into();

        assert_eq!(weather.today_index, 2, "two days precede today");
    }

    #[test]
    fn supplementary_readings_survive_being_absent() {
        let json = r#"{
            "current": {"time": "2026-08-09T14:30", "temperature_2m": 20.0,
                        "apparent_temperature": 20.0, "weather_code": 0, "wind_speed_10m": 5.0},
            "daily": {
                "time": ["2026-08-09"], "weather_code": [0],
                "temperature_2m_max": [30.0], "temperature_2m_min": [20.0]
            }
        }"#;

        let weather: Weather = serde_json::from_str::<ForecastDto>(json).unwrap().into();
        let day = &weather.daily[0];

        assert_eq!(weather.daily.len(), 1, "a day is kept without its extras");
        assert!(day.uv_index.is_none());
        assert!(day.precip_mm.is_none());
        assert!(day.daylight_secs.is_none());
    }

    /// The endpoint has no daily aggregation, so per-day figures come from
    /// collapsing the hourly series. The maximum is what matters — an average
    /// would hide the hour the air was actually bad.
    #[test]
    fn hourly_air_quality_collapses_to_a_daily_maximum() {
        let json = r#"{
            "current": {"us_aqi": 68},
            "hourly": {
                "time": ["2026-08-10T00:00", "2026-08-10T01:00", "2026-08-10T02:00",
                         "2026-08-11T00:00", "2026-08-11T01:00"],
                "us_aqi": [40, 102, null, 55, 51]
            }
        }"#;

        let report: AirQualityReport = serde_json::from_str::<AqiDto>(json).unwrap().into();

        assert_eq!(report.current.map(|c| c.us_aqi), Some(68));
        assert_eq!(report.daily_max.get("2026-08-10").copied(), Some(102));
        assert_eq!(report.daily_max.get("2026-08-11").copied(), Some(55));
        assert_eq!(
            report.daily_max.get("2026-08-12"),
            None,
            "uncovered days absent"
        );
    }

    #[test]
    fn air_quality_survives_an_absent_hourly_series() {
        let json = r#"{"current": {"us_aqi": 42}}"#;
        let report: AirQualityReport = serde_json::from_str::<AqiDto>(json).unwrap().into();

        assert_eq!(report.current.map(|c| c.us_aqi), Some(42));
        assert!(report.daily_max.is_empty());
    }

    /// Builds a forecast whose `current.time` sits at 02:30, with four hourly
    /// points either side of it, so tests can vary just the hourly block.
    fn with_hourly(hourly: &str) -> Weather {
        let json = format!(
            r#"{{
                "current": {{"time": "2026-08-09T02:30", "temperature_2m": 20.0,
                             "apparent_temperature": 20.0, "weather_code": 0, "wind_speed_10m": 5.0}},
                "daily": {{
                    "time": ["2026-08-09"], "weather_code": [0],
                    "temperature_2m_max": [30.0], "temperature_2m_min": [20.0]
                }},
                "hourly": {hourly}
            }}"#
        );
        serde_json::from_str::<ForecastDto>(&json)
            .expect("hourly fixture should parse")
            .into()
    }

    const FOUR_HOURS: &str = r#"{
        "time": ["2026-08-09T00:00", "2026-08-09T01:00", "2026-08-09T02:00", "2026-08-09T03:00"],
        "precipitation": [0.0, 0.2, null, 1.4],
        "precipitation_probability": [5, 40, null, 90],
        "snowfall": [0.0, 0.0, 0.0, 0.7],
        "weather_code": [0, 61, null, 71],
        "temperature_2m": [18.0, 17.5, null, 16.0],
        "apparent_temperature": [17.0, 16.5, null, 15.0],
        "relative_humidity_2m": [55, 60, null, 70],
        "wind_speed_10m": [5.0, 10.0, null, 20.0],
        "wind_gusts_10m": [8.0, 15.0, null, 30.0],
        "wind_direction_10m": [0.0, 90.0, null, 225.0]
    }"#;

    #[test]
    fn hourly_conditions_are_mapped_by_timestamp_index() {
        let weather = with_hourly(FOUR_HOURS);
        let hour = &weather.hourly[1];

        assert_eq!(hour.time, "2026-08-09T01:00");
        assert_eq!(hour.feels_like_c, Some(16.5));
        assert_eq!(hour.humidity_pct, Some(60));
        assert_eq!(hour.wind_kph, Some(10.0));
        assert_eq!(hour.gust_kph, Some(15.0));
        assert_eq!(hour.wind_dir_deg, Some(90.0));
    }

    /// A null reading blanks that one cell. Unlike a daily row there is no
    /// required measurement, so no hour is ever dropped — which is what keeps
    /// `now_hour` addressable by position.
    #[test]
    fn a_null_hourly_reading_blanks_the_cell_rather_than_the_hour() {
        let weather = with_hourly(FOUR_HOURS);

        assert_eq!(weather.hourly.len(), 4, "the null hour is kept");
        let null_hour = &weather.hourly[2];
        assert_eq!(null_hour.time, "2026-08-09T02:00");
        assert!(null_hour.precip_mm.is_none());
        assert!(null_hour.chance.is_none());
        assert!(null_hour.temp_c.is_none());
    }

    /// Snow is carried separately from precipitation because they measure the
    /// same hour differently — 1.4 mm melted is 0.7 cm fallen.
    #[test]
    fn snowfall_is_kept_apart_from_precipitation() {
        let weather = with_hourly(FOUR_HOURS);
        let snowy = &weather.hourly[3];

        assert_eq!(snowy.precip_mm, Some(1.4));
        assert_eq!(snowy.snow_cm, Some(0.7));
        assert!(snowy.is_snow(), "0.7 cm is snow");
        assert!(!weather.hourly[1].is_snow(), "0.2 mm of rain is not");
    }

    /// A stripped request or an older cached fixture has no hourly block at
    /// all. That must degrade to an empty screen, not a failed forecast.
    #[test]
    fn an_absent_hourly_block_still_yields_a_forecast() {
        let json = r#"{
            "current": {"time": "2026-08-09T02:30", "temperature_2m": 20.0,
                        "apparent_temperature": 20.0, "weather_code": 0, "wind_speed_10m": 5.0},
            "daily": {"time": ["2026-08-09"], "weather_code": [0],
                      "temperature_2m_max": [30.0], "temperature_2m_min": [20.0]}
        }"#;

        let weather: Weather = serde_json::from_str::<ForecastDto>(json).unwrap().into();

        assert!(weather.hourly.is_empty());
        assert_eq!(weather.now_hour, 0);
        assert!(weather.forecast_hours().is_empty(), "and slices safely");
        assert_eq!(weather.daily.len(), 1, "the daily forecast still arrives");

        let old_fixture = with_hourly(
            r#"{
                "time": ["2026-08-09T00:00"],
                "temperature_2m": [18.0]
            }"#,
        );
        assert_eq!(old_fixture.hourly.len(), 1);
        assert!(old_fixture.hourly[0].humidity_pct.is_none());
        assert!(old_fixture.hourly[0].wind_kph.is_none());
    }

    /// Arrays shorter than `time` blank the tail rather than truncating the
    /// series or panicking on the index.
    #[test]
    fn mismatched_hourly_arrays_blank_the_missing_readings() {
        let weather = with_hourly(
            r#"{
                "time": ["2026-08-09T00:00", "2026-08-09T01:00", "2026-08-09T02:00"],
                "precipitation": [0.0],
                "precipitation_probability": [5, 40]
            }"#,
        );

        assert_eq!(weather.hourly.len(), 3, "time drives the length");
        assert_eq!(weather.hourly[0].precip_mm, Some(0.0));
        assert!(weather.hourly[1].precip_mm.is_none());
        assert_eq!(weather.hourly[1].chance, Some(40));
        assert!(weather.hourly[2].chance.is_none());
        assert!(weather.hourly[2].snow_cm.is_none(), "absent array too");

        let weather = with_hourly(
            r#"{
                "time": ["2026-08-09T00:00", "2026-08-09T01:00"],
                "relative_humidity_2m": [55]
            }"#,
        );
        assert_eq!(weather.hourly.len(), 2);
        assert_eq!(weather.hourly[0].humidity_pct, Some(55));
        assert_eq!(weather.hourly[1].humidity_pct, None);
    }

    /// The series runs two weeks into the past, so an off-by-one here points
    /// the whole screen at history. 02:30 belongs to the 02:00 hour.
    #[test]
    fn now_hour_points_at_the_current_hour() {
        let weather = with_hourly(FOUR_HOURS);

        assert_eq!(weather.now_hour, 2);
        assert_eq!(weather.hourly[weather.now_hour].time, "2026-08-09T02:00");
    }

    /// If the current hour is missing from the series, land on the boundary
    /// rather than at zero — which would present two weeks of history as the
    /// forecast.
    #[test]
    fn now_hour_falls_back_to_the_boundary_when_its_hour_is_absent() {
        let weather = with_hourly(
            r#"{
                "time": ["2026-08-09T00:00", "2026-08-09T01:00", "2026-08-09T03:00"],
                "precipitation": [0.0, 0.0, 0.0]
            }"#,
        );

        assert_eq!(weather.now_hour, 2, "two hours precede 02:00");
        assert_eq!(
            weather.forecast_hours()[0].time,
            "2026-08-09T03:00",
            "the window opens at the next hour that exists"
        );
    }

    /// The screen looks only forward; the request carries history for the
    /// daily chart's sake.
    #[test]
    fn forecast_hours_start_at_now_and_drop_the_past() {
        let weather = with_hourly(FOUR_HOURS);
        let forward = weather.forecast_hours();

        assert_eq!(forward.len(), 2, "02:00 and 03:00");
        assert_eq!(forward[0].time, "2026-08-09T02:00");
        assert!(
            forward
                .iter()
                .all(|h| h.time.as_str() >= "2026-08-09T02:00"),
            "nothing before now survives the slice"
        );
    }

    #[test]
    fn parses_geocode_results() {
        let json = include_str!("../../tests/fixtures/geocode.json");
        let dto: GeocodeDto = serde_json::from_str(json).expect("should parse");
        assert_eq!(dto.results[0].name, "Frederick");
        assert_eq!(dto.results[0].admin1.as_deref(), Some("Maryland"));
    }

    #[test]
    fn parses_empty_geocode_results() {
        let json = include_str!("../../tests/fixtures/geocode_empty.json");
        let dto: GeocodeDto = serde_json::from_str(json).expect("should parse");
        assert!(dto.results.is_empty());
    }
}
