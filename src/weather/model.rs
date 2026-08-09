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
    pub daily: Vec<DailyForecast>,
    pub air_quality: Option<AirQuality>,
}

pub struct AirQuality {
    pub us_aqi: u16,
}

impl Weather {
    #[cfg(test)]
    pub fn sample() -> Self {
        Self {
            location: "".to_string(),
            current: Current {
                temp_c: 30.0,
                feels_like_c: 30.0,
                code: 1,
                wind_kph: 5.0,
            },
            daily: vec![
                DailyForecast { date: "Mon".to_string(), high_c: 32.0, low_c: 28.0, code: 1 },
                DailyForecast { date: "Tue".to_string(), high_c: 35.0, low_c: 30.0, code: 1 },
                DailyForecast { date: "Wed".to_string(), high_c: 30.0, low_c: 25.0, code: 1 },
                DailyForecast { date: "Thu".to_string(), high_c: 28.0, low_c: 23.0, code: 1 },
                DailyForecast { date: "Fri".to_string(), high_c: 26.0, low_c: 22.0, code: 1 },
            ],
            air_quality: None,
        }
    }
}
