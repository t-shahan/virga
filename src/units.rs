#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Unit {
    Metric,
    Imperial,
}

impl Unit {
    /// The system a name asks for, if it names one. Both systems answer to
    /// their proper name, their temperature scale, and that scale's initial —
    /// `VIRGA_UNITS=c` is what someone in a hurry will type, and refusing it
    /// over a spelling rule would help nobody.
    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "metric" | "celsius" | "c" => Some(Unit::Metric),
            "imperial" | "fahrenheit" | "f" => Some(Unit::Imperial),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Unit::Metric => "metric",
            Unit::Imperial => "imperial",
        }
    }

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

    /// The temperature as the whole number every display prints it as.
    ///
    /// Not merely `temp(..)` under `{:.0}`: a reading in (-0.5, 0] rounds to
    /// negative zero, and `{:.0}` prints that as `-0`. No thermometer says
    /// `-0°`, and a script parsing `virga now` may treat it as a value
    /// distinct from `0`. Adding `0.0` is the standard way to lose the sign,
    /// since `-0.0 + 0.0` is `+0.0` under IEEE 754.
    ///
    /// Ties round to even, not away from zero, because that is what `{:.0}`
    /// has always done here and Open-Meteo reports tenths: `f64::round`
    /// would have moved every `.5` reading a degree from where 0.6.0 put it.
    pub fn temp_rounded(self, celsius: f64) -> f64 {
        self.temp(celsius).round_ties_even() + 0.0
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

    /// Snow is reported in centimetres, where rain is in millimetres — the
    /// same hour can carry 0.7 cm of snow and 1.4 mm of precipitation.
    pub fn snow(self, cm: f64) -> f64 {
        match self {
            Unit::Metric => cm,
            Unit::Imperial => cm / 2.54,
        }
    }

    /// A tenth of an inch of snow is already visible on the ground, so snow
    /// needs one fewer decimal than rain does.
    pub fn snow_decimals(self) -> usize {
        1
    }

    pub fn snow_label(self) -> &'static str {
        match self {
            Unit::Metric => "cm",
            Unit::Imperial => "in",
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
    fn every_accepted_spelling_names_its_system() {
        for name in ["metric", "celsius", "c", " Metric ", "CELSIUS"] {
            assert_eq!(Unit::from_name(name), Some(Unit::Metric), "{name:?}");
        }
        for name in ["imperial", "fahrenheit", "f", " Imperial ", "F"] {
            assert_eq!(Unit::from_name(name), Some(Unit::Imperial), "{name:?}");
        }
    }

    #[test]
    fn a_name_that_is_neither_system_is_refused() {
        for name in ["", "  ", "kelvin", "k", "si", "us", "both"] {
            assert_eq!(Unit::from_name(name), None, "{name:?}");
        }
    }

    #[test]
    fn names_round_trip_through_parsing() {
        for unit in [Unit::Metric, Unit::Imperial] {
            assert_eq!(Unit::from_name(unit.name()), Some(unit));
        }
    }

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
        assert_eq!(Unit::Metric.snow_label(), "cm");
        assert_eq!(Unit::Imperial.snow_label(), "in");
    }

    /// Snow converts from centimetres, so reusing the rain conversion would
    /// under-report it by a factor of ten.
    #[test]
    fn snow_converts_from_centimetres_not_millimetres() {
        assert_eq!(Unit::Metric.snow(2.5), 2.5);
        assert!((Unit::Imperial.snow(2.54) - 1.0).abs() < 0.000_1);
        assert!(
            Unit::Imperial.snow(2.54) > Unit::Imperial.precip(2.54),
            "a centimetre is not a millimetre"
        );
    }

    /// The trap is the half-degree below zero in whichever scale is shown:
    /// -0.3 °C in metric, and -18 °C in imperial, which is -0.4 °F. Both
    /// used to print as `-0`. A negative-zero reading itself, which JSON can
    /// carry, is the same trap without the rounding.
    #[test]
    fn a_temperature_just_below_zero_rounds_to_zero_not_negative_zero() {
        for (unit, celsius) in [
            (Unit::Metric, -0.3),
            (Unit::Metric, -0.5),
            (Unit::Metric, -0.0),
            (Unit::Imperial, -18.0),
        ] {
            let rounded = unit.temp_rounded(celsius);
            assert!(
                !rounded.is_sign_negative(),
                "{unit:?} {celsius} gave {rounded}"
            );
            assert_eq!(format!("{rounded:.0}"), "0", "{unit:?} {celsius}");
        }
    }

    /// Only the sign of zero changes. A reading genuinely below zero keeps
    /// its sign, the conversion still applies, and a tie still rounds to
    /// even the way `{:.0}` always has, so 22.5 stays `22` rather than
    /// moving to `23` when the helper took over.
    #[test]
    fn rounding_keeps_a_real_negative_the_conversion_and_the_tie_rule() {
        assert_eq!(Unit::Metric.temp_rounded(-0.6), -1.0);
        assert_eq!(Unit::Metric.temp_rounded(-2.6), -3.0);
        assert_eq!(Unit::Imperial.temp_rounded(-40.0), -40.0);
        assert_eq!(Unit::Imperial.temp_rounded(21.5), 71.0);
        for (value, printed) in [(22.5, "22"), (23.5, "24"), (2.5, "2"), (-2.5, "-2")] {
            let rounded = Unit::Metric.temp_rounded(value);
            assert_eq!(format!("{rounded:.0}"), printed, "{value}");
            assert_eq!(format!("{value:.0}"), printed, "the rule `{{:.0}}` uses");
        }
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
