use crate::weather::model::{Current, DailyForecast, Location, Weather};
use chrono::Local;
use serde::Deserialize;

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
}

#[derive(Debug, Deserialize)]
pub struct ForecastDto {
    #[serde(default)]
    pub timezone: String,
    pub current: CurrentDto,
    pub daily: DailyDto,
}

/// `values[i]`, collapsing "index out of range" and "value was null" into the
/// same `None` — the caller cares about neither distinction.
fn at<T: Copy>(values: &[Option<T>], i: usize) -> Option<T> {
    values.get(i).copied().flatten()
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
                })
            })
            .collect();

        // Located after filtering, so a dropped row can't shift the index. If
        // today's own row was dropped, fall back to the count of days before it.
        let today_index = daily
            .iter()
            .position(|day| day.date == today)
            .unwrap_or_else(|| daily.iter().filter(|day| day.date < today).count());

        Self {
            location: dto.timezone,
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
    /// should still come through — a blank "Now" pane beats no weather at all.
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
