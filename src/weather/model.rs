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
    pub code: u8,
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

impl AirQuality {
    pub fn new(us_aqi: u16) -> Self {
        Self { us_aqi }
    }
}

impl DailyForecast {
    pub fn new(date: String, high_c: f64, low_c: f64, code: u8) -> Self {
        Self {
            date,
            high_c,
            low_c,
            code,
        }
    }
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
                DailyForecast::new("Mon".to_string(), 32.0, 28.0, 1),
                DailyForecast::new("Tue".to_string(), 35.0, 30.0, 1),
                DailyForecast::new("Wed".to_string(), 30.0, 25.0, 1),
                DailyForecast::new("Thu".to_string(), 28.0, 23.0, 1),
                DailyForecast::new("Fri".to_string(), 26.0, 22.0, 1),
            ],
            air_quality: None,
        }
    }
}
