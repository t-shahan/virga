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

pub struct Current {
    pub temp_c: f64,
    pub feels_like_c: f64,
    pub code: u8,
    pub wind_kph: f64,
}

pub struct DailyForecast {
    pub date: String,
    pub high_c: f64,
    pub low_c: f64,
    pub code: u8, // Holding on to this for later when adding emoji() in the 5 day forecast
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
