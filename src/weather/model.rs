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
pub struct Current {
    pub temp_c: Option<f64>,
    pub feels_like_c: Option<f64>,
    pub code: Option<u8>,
    pub wind_kph: Option<f64>,
}

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

pub struct Weather {
    pub current: Current,
    /// Past days, today, then the forecast — in chronological order.
    pub daily: Vec<DailyForecast>,
    /// Index of today within `daily`.
    pub today_index: usize,
    pub air_quality: Option<AirQuality>,
}

/// Everything one air-quality request yields: the live reading, plus a maximum
/// for each date it covers. The endpoint has no daily aggregation, so the
/// per-day figures are derived from its hourly series.
#[derive(Default)]
pub struct AirQualityReport {
    pub current: Option<AirQuality>,
    pub daily_max: HashMap<String, u16>,
}

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
            air_quality: None,
        }
    }
}
