#[derive(Clone, Copy, PartialEq, Debug)]
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
    pub fn precip(self, mm: f64) -> f64 {
        match self {
            Unit::Metric => mm,
            Unit::Imperial => mm / 25.4,
        }
    }

    /// Hundredths of an inch are a meaningful amount of rain; hundredths of a
    /// millimetre are noise, and the extra digit costs column width.
    pub fn precip_decimals(self) -> usize {
        match self {
            Unit::Metric => 1,
            Unit::Imperial => 2,
        }
    }

    pub fn precip_label(self) -> &'static str {
        match self {
            Unit::Metric => "mm",
            Unit::Imperial => "in",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn celsius_to_fahrenheit_at_known_points() {
        assert_eq!(c_to_f(0.0), 32.0);
        assert_eq!(c_to_f(100.0), 212.0);
        assert_eq!(c_to_f(-40.0), -40.0);
    }

    #[test]
    fn metric_passes_values_through_untouched() {
        assert_eq!(Unit::Metric.temp(21.5), 21.5);
        assert_eq!(Unit::Metric.speed(30.0), 30.0);
        assert_eq!(Unit::Metric.precip(5.0), 5.0);
    }

    #[test]
    fn imperial_converts_every_measure() {
        assert_eq!(Unit::Imperial.temp(0.0), 32.0);
        assert!((Unit::Imperial.speed(100.0) - 62.1371).abs() < 0.001);
        assert!((Unit::Imperial.precip(25.4) - 1.0).abs() < 0.000_1);
    }

    #[test]
    fn labels_match_the_system() {
        assert_eq!(Unit::Metric.temp_symbol(), "\u{b0}C");
        assert_eq!(Unit::Imperial.temp_symbol(), "\u{b0}F");
        assert_eq!(Unit::Metric.speed_label(), "km/h");
        assert_eq!(Unit::Imperial.speed_label(), "mph");
        assert_eq!(Unit::Metric.precip_label(), "mm");
        assert_eq!(Unit::Imperial.precip_label(), "in");
    }

    #[test]
    fn toggling_twice_is_the_identity() {
        assert_eq!(Unit::Metric.toggle().toggle(), Unit::Metric);
        assert_eq!(Unit::Imperial.toggle().toggle(), Unit::Imperial);
        assert_eq!(Unit::Metric.toggle(), Unit::Imperial);
    }

    /// The unit symbol is two cells wide; the block-digit rows are padded to
    /// match it, so a change here would shear the hero temperature.
    #[test]
    fn temp_symbols_are_two_characters() {
        assert_eq!(Unit::Metric.temp_symbol().chars().count(), 2);
        assert_eq!(Unit::Imperial.temp_symbol().chars().count(), 2);
    }
}
