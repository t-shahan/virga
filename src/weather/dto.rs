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

#[derive(Debug, Deserialize)]
pub struct DailyDto {
    pub time: Vec<String>,
    pub weather_code: Vec<u8>,
    pub temperature_2m_max: Vec<f64>,
    pub temperature_2m_min: Vec<f64>,
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
        let today_index = dto
            .daily
            .time
            .iter()
            .position(|day| *day == today)
            .unwrap_or(0);

        let daily = dto
            .daily
            .time
            .into_iter()
            .zip(dto.daily.temperature_2m_max)
            .zip(dto.daily.temperature_2m_min)
            .zip(dto.daily.weather_code)
            .map(|(((date, high), low), code)| DailyForecast {
                date,
                high_c: high,
                low_c: low,
                code,
            })
            .collect();

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
