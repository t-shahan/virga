use crate::weather::dto::AqiDto;
use crate::weather::dto::ForecastDto;
use crate::weather::dto::GeocodeDto;
use crate::weather::model::AirQuality;
use crate::weather::model::Location;
use crate::weather::model::Weather;
use anyhow::Result;
use std::thread;

pub fn fetch_forecast(lat: f64, lon: f64) -> Result<Weather> {
    let aqi = thread::spawn(move || fetch_air_quality(lat, lon));

    let mut response = ureq::get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query(
            "current",
            "temperature_2m,apparent_temperature,weather_code,wind_speed_10m",
        )
        .query(
            "daily",
            "weather_code,temperature_2m_max,temperature_2m_min",
        )
        .query("timezone", "auto")
        .query("forecast_days", "8")
        .query("past_days", "14")
        .call()?;
    let dto: ForecastDto = response.body_mut().read_json()?;
    let mut weather: Weather = dto.into();

    weather.air_quality = match aqi.join() {
        Ok(Ok(aq)) => aq,
        _ => None,
    };

    Ok(weather)
}

pub fn search_locations(query: &str) -> Result<Vec<Location>> {
    let mut response = ureq::get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", query)
        .query("count", "5")
        .query("language", "en")
        .query("format", "json")
        .call()?;

    let dto: GeocodeDto = response.body_mut().read_json()?;

    Ok(dto.results.into_iter().map(Location::from).collect())
}

pub fn fetch_air_quality(lat: f64, lon: f64) -> Result<Option<AirQuality>> {
    let url = format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality?latitude={lat}&longitude={lon}&hourly=us_aqi&current=us_aqi&domains=cams_global"
    );

    let mut response = ureq::get(&url).call()?;
    let dto: AqiDto = response.body_mut().read_json()?;

    Ok(dto.current.us_aqi.map(|us_aqi| AirQuality { us_aqi }))
}
