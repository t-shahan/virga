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
    pub location: String,
    pub current: Current,
    /// Past days, today, then the forecast — in chronological order.
    pub daily: Vec<DailyForecast>,
    /// Index of today within `daily`.
    pub today_index: usize,
    pub air_quality: Option<AirQuality>,
}

pub struct AirQuality {
    pub us_aqi: u16,
}
