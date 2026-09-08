/// One phrase per WMO code rather than a range shared across several. The old
/// lumped strings were both vague ("Slight to violent rain showers" for four
/// different codes) and too wide for the pane that shows them.
pub fn description(code: u8) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Rime fog",
        51 => "Light drizzle",
        53 => "Drizzle",
        55 => "Heavy drizzle",
        56 => "Freezing drizzle",
        57 => "Heavy freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 => "Freezing rain",
        67 => "Heavy freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 => "Light showers",
        81 => "Showers",
        82 => "Violent showers",
        85 => "Light snow showers",
        86 => "Heavy snow showers",
        95 => "Thunderstorm",
        96 => "Thunderstorm, hail",
        99 => "Thunderstorm, heavy hail",
        _ => "Unknown",
    }
}

pub fn emoji(code: u8) -> &'static str {
    match code {
        0 => "☀️",
        1 => "🌤️",
        2 => "⛅️",
        3 => "☁️",
        45 => "🌫️",
        48 => "🌫️",
        51 => "🌧️",
        56 => "🌧️",
        66 => "🌧️",
        81 => "🌧️",
        53 => "🌧️",
        55 => "🌧️",
        57 => "🌧️",
        61 => "🌧️",
        63 => "🌧️",
        65 => "🌧️",
        67 => "🌧️",
        71 => "🌨️",
        73 => "🌨️",
        75 => "🌨️",
        77 => "🌨️",
        80 => "🌧️",
        82 => "🌧️",
        85 => "🌧️",
        86 => "🌧️",
        95 => "⛈️",
        96 => "⛈️",
        99 => "⛈️",
        _ => "❓",
    }
}

pub fn aqi_label(aqi: u16) -> &'static str {
    match aqi {
        0..=50 => "Good",
        51..=100 => "Moderate",
        101..=150 => "Unhealthy for Sensitive Groups",
        151..=200 => "Unhealthy",
        201..=300 => "Very Unhealthy",
        _ => "Hazardous",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bands were wrong once already, and boundaries are where that kind of
    /// mistake lives — so every band is checked on both sides.
    #[test]
    fn aqi_bands_break_on_the_epa_boundaries() {
        assert_eq!(aqi_label(0), "Good");
        assert_eq!(aqi_label(50), "Good");
        assert_eq!(aqi_label(51), "Moderate");
        assert_eq!(aqi_label(100), "Moderate");
        assert_eq!(aqi_label(101), "Unhealthy for Sensitive Groups");
        assert_eq!(aqi_label(150), "Unhealthy for Sensitive Groups");
        assert_eq!(aqi_label(151), "Unhealthy");
        assert_eq!(aqi_label(200), "Unhealthy");
        assert_eq!(aqi_label(201), "Very Unhealthy");
        assert_eq!(aqi_label(300), "Very Unhealthy");
        assert_eq!(aqi_label(301), "Hazardous");
        assert_eq!(aqi_label(500), "Hazardous");
    }

    /// AQI is a u16 precisely because it can exceed 255.
    #[test]
    fn aqi_handles_values_above_a_byte() {
        assert_eq!(aqi_label(400), "Hazardous");
    }

    /// Every code the `description` match names. Kept as a list rather than
    /// derived from the function, because the point is to catch a code the
    /// match has quietly lost: a test that asks the function what it covers
    /// can only ever agree with it.
    const DOCUMENTED: [u8; 28] = [
        0, 1, 2, 3, 45, 48, 51, 53, 55, 56, 57, 61, 63, 65, 66, 67, 71, 73, 75, 77, 80, 81, 82, 85,
        86, 95, 96, 99,
    ];

    /// The old form of this test asserted the strings were non-empty, which
    /// the fallback arm satisfies for every code, so nothing could fail it.
    /// Comparing against the fallback is the assertion that has teeth.
    #[test]
    fn every_documented_code_has_its_own_description_and_icon() {
        let unknown = description(200);
        let no_icon = emoji(200);
        for code in DOCUMENTED {
            assert_ne!(description(code), unknown, "code {code} fell through");
            assert_ne!(emoji(code), no_icon, "code {code} has no icon of its own");
        }
    }

    /// The other direction: a code the table does not know must say so rather
    /// than borrow a neighbour's phrase, and the same fallback must serve the
    /// whole byte, not just the range WMO uses.
    #[test]
    fn undocumented_codes_all_read_as_unknown() {
        let unknown = description(200);
        let no_icon = emoji(200);
        for code in (0..=u8::MAX).filter(|code| !DOCUMENTED.contains(code)) {
            assert_eq!(description(code), unknown, "code {code} claimed a phrase");
            assert_eq!(emoji(code), no_icon, "code {code} claimed an icon");
        }
    }

    /// One phrase per code is the contract at the top of the file; two codes
    /// sharing a phrase is the lumping it replaced creeping back.
    #[test]
    fn documented_descriptions_are_distinct() {
        for (i, a) in DOCUMENTED.iter().enumerate() {
            for b in &DOCUMENTED[i + 1..] {
                assert_ne!(
                    description(*a),
                    description(*b),
                    "codes {a} and {b} share a description"
                );
            }
        }
    }
}
