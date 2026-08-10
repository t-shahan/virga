use crate::weather::dto::AqiDto;
use crate::weather::dto::ForecastDto;
use crate::weather::dto::GeocodeDto;
use crate::weather::dto::GeocodeResultDto;
use crate::weather::model::AirQualityReport;
use crate::weather::model::Location;
use crate::weather::model::Weather;
use anyhow::Result;
use std::sync::OnceLock;
use std::thread;
use ureq::Agent;

/// One shared agent for the whole process. `ureq::get()` builds a fresh Agent
/// per call, and a fresh Agent means a fresh connection pool — so every request
/// re-paid a full TCP + TLS handshake. Measured against Open-Meteo that was
/// ~360ms of the ~480ms round trip. Sharing one agent lets repeat requests to
/// the same host reuse the connection.
fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(Agent::new_with_defaults)
}

pub fn fetch_forecast(lat: f64, lon: f64) -> Result<Weather> {
    let aqi = thread::spawn(move || fetch_air_quality(lat, lon));

    let mut response = agent().get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query(
            "current",
            "temperature_2m,apparent_temperature,weather_code,wind_speed_10m",
        )
        .query(
            "daily",
            "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,wind_speed_10m_max,uv_index_max,sunrise,sunset,apparent_temperature_max,apparent_temperature_min,precipitation_sum,precipitation_hours,wind_gusts_10m_max,wind_direction_10m_dominant,daylight_duration",
        )
        .query("timezone", "auto")
        .query("forecast_days", "8")
        .query("past_days", "14")
        .call()?;
    let dto: ForecastDto = response.body_mut().read_json()?;
    let mut weather: Weather = dto.into();

    let report = match aqi.join() {
        Ok(Ok(report)) => report,
        _ => AirQualityReport::default(),
    };

    weather.air_quality = report.current;
    for day in &mut weather.daily {
        day.aqi = report.daily_max.get(&day.date).copied();
    }

    Ok(weather)
}

pub fn search_locations(query: &str) -> Result<Vec<Location>> {
    let mut response = agent()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", query)
        .query("count", "5")
        .query("language", "en")
        .query("format", "json")
        .call()?;

    let dto: GeocodeDto = response.body_mut().read_json()?;

    Ok(dto
        .results
        .into_iter()
        .filter_map(GeocodeResultDto::into_location)
        .collect())
}

pub fn fetch_air_quality(lat: f64, lon: f64) -> Result<AirQualityReport> {
    // The window matches the forecast request so nearly every day the user can
    // browse to carries a figure. Coverage runs out a couple of days short of
    // the forecast horizon, which the UI shows as absence rather than zero.
    let mut response = agent()
        .get("https://air-quality-api.open-meteo.com/v1/air-quality")
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query("current", "us_aqi")
        .query("hourly", "us_aqi")
        .query("timezone", "auto")
        .query("past_days", "14")
        .query("forecast_days", "7")
        .query("domains", "cams_global")
        .call()?;

    let dto: AqiDto = response.body_mut().read_json()?;

    Ok(dto.into())
}

#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn real_fetch_carries_per_day_air_quality() {
        let weather = fetch_forecast(39.41427, -77.41054).expect("fetch");
        let covered = weather.daily.iter().filter(|d| d.aqi.is_some()).count();

        println!(
            "current AQI: {:?}",
            weather.air_quality.as_ref().map(|a| a.us_aqi)
        );
        println!("days: {}  with AQI: {covered}", weather.daily.len());
        for d in &weather.daily {
            println!("  {}  high {:>5.1}C  aqi {:?}", d.date, d.high_c, d.aqi);
        }
        assert!(
            covered > 15,
            "expected most of the window covered, got {covered}"
        );
    }
}
