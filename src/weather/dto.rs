use crate::weather::model::{Current, DailyForecast, Location, Weather};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GeocodeResultDto {
    pub name: String,
    pub admin1: Option<String>,
    pub country: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Deserialize)]
pub struct GeocodeDto {
    #[serde(default)]
    pub results: Vec<GeocodeResultDto>,
}

impl From<GeocodeResultDto> for Location {
    fn from(dto: GeocodeResultDto) -> Self {
        Self {
            name: dto.name,
            admin1: dto.admin1,
            country: dto.country,
            lat: dto.latitude,
            lon: dto.longitude,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CurrentDto {
    pub time: String, // Keeping this to add a 'Last Updated: {time}' field to the current weather
    pub temperature_2m: f64,
    pub apparent_temperature: f64,
    pub weather_code: u8,
    pub wind_speed_10m: f64,
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
}

#[derive(Debug, Deserialize)]
pub struct ForecastDto {
    pub timezone: String,
    pub current: CurrentDto,
    pub daily: DailyDto,
}

impl From<ForecastDto> for Weather {
    fn from(dto: ForecastDto) -> Self {
        // `current.time` is local to the forecast location, so its date identifies
        // today's entry without assuming how many past days were requested.
        let today = dto.current.time.get(..10).unwrap_or_default().to_string();

        // `?` inside filter_map drops any day missing a measurement instead of
        // failing the entire forecast.
        let daily: Vec<DailyForecast> = dto
            .daily
            .time
            .into_iter()
            .zip(dto.daily.temperature_2m_max)
            .zip(dto.daily.temperature_2m_min)
            .zip(dto.daily.weather_code)
            .filter_map(|(((date, high), low), code)| {
                Some(DailyForecast {
                    date,
                    high_c: high?,
                    low_c: low?,
                    code: code?,
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
    pub current: AqiCurrentDto,
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
