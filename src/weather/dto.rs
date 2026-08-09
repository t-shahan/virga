use serde::Deserialize;
use crate::weather::model::{Current, DailyForecast, Location, Weather, AirQuality};

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
    pub time: String,
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

    fn fixture() -> ForecastDto {
        let json = include_str!("../../tests/fixtures/forecast.json");
        serde_json::from_str(json).expect("fixture should parse")
    }

    #[test]
    fn parses_saved_fixture() {
        let dto = fixture();

        assert_eq!(dto.daily.time.len(), 5);
        assert_eq!(dto.daily.weather_code.len(), 5);
        assert_eq!(dto.daily.temperature_2m_max.len(), 5);
        assert_eq!(dto.daily.temperature_2m_min.len(), 5);
    }

    #[test]
    fn converts_dto_into_domain_weather() {
        let weather: Weather = fixture().into();

        assert_eq!(weather.daily.len(), 5);
        assert_eq!(weather.daily[0].high_c, 36.2);
        assert_eq!(weather.current.temp_c, 26.8);
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
