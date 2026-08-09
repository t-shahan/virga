#[derive(Clone, Copy, PartialEq)]
pub enum Unit {
    Metric,
    Imperial,
}

impl Unit {
    pub fn toggle(self) -> Self {
        match self {
            Unit::Metric => Unit::Imperial,
            Unit::Imperial => Unit::Metric,
        }
    }
    pub fn temp(self, celsius: f64) -> f64 {
        match self {
            Unit::Metric => celsius,
            Unit::Imperial => c_to_f(celsius),
        }
    }
    pub fn temp_symbol(self) -> &'static str {
        match self {
            Unit::Metric => "°C",
            Unit::Imperial => "°F",
        }
    }
    pub fn speed(self, kph: f64) -> f64 {
        match self {
            Unit::Metric => kph,
            Unit::Imperial => kph_to_mph(kph),
        }
    }
    pub fn speed_label(self) -> &'static str {
        match self {
            Unit::Metric => "km/h",
            Unit::Imperial => "mph",
        }
    }
}

pub fn c_to_f(c: f64) -> f64 {
    c * 9.0 / 5.0 + 32.0
}

pub fn kph_to_mph(kph: f64) -> f64 {
    kph * 0.621371
}
