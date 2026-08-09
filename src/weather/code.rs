pub fn description(code: u8) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mainly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 => "Fog",
        48 => "Depositing rime fog",
        51 => "Light to dense drizzle",
        53 => "Light to dense drizzle",
        55 => "Light to dense drizzle",
        57 => "Light to dense drizzle",
        61 => "Slight to heavy rain",
        63 => "Slight to heavy rain",
        65 => "Slight to heavy rain",
        67 => "Slight to heavy rain",
        71 => "Slight to heavy snowfall",
        73 => "Slight to heavy snowfall",
        75 => "Slight to heavy snowfall",
        77 => "Slight to heavy snowfall",
        80 => "Slight to violent rain showers",
        82 => "Slight to violent rain showers",
        85 => "Slight to violent rain showers",
        86 => "Slight to violent rain showers",
        95 => "Slight/moderate thunderstorms",
        96 => "Slight/moderate thunderstorms",
        99 => "Slight/moderate thunderstorms",
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
