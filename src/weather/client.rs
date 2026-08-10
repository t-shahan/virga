use crate::weather::dto::AqiDto;
use crate::weather::dto::ForecastDto;
use crate::weather::dto::GeocodeDto;
use crate::weather::dto::GeocodeResultDto;
use crate::weather::model::AirQualityReport;
use crate::weather::model::Location;
use crate::weather::model::Weather;
use anyhow::Result;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use ureq::Agent;

/// End-to-end bound on a request: DNS, connect, TLS, response and body read.
/// `ureq` defaults every one of these to `None`, so before this a hung server
/// blocked the sole worker thread forever and the UI sat on "Loading…" with no
/// way back. Long enough for a slow mobile connection, short enough that a
/// wedged request fails while the user is still waiting for it.
const TIMEOUT_GLOBAL: Duration = Duration::from_secs(15);
/// Reaching an unreachable host should not burn the whole global budget before
/// saying so.
const TIMEOUT_CONNECT: Duration = Duration::from_secs(5);

/// One shared agent for the whole process. `ureq::get()` builds a fresh Agent
/// per call, and a fresh Agent means a fresh connection pool — so every request
/// re-paid a full TCP + TLS handshake. Measured against Open-Meteo that was
/// ~360ms of the ~480ms round trip. Sharing one agent lets repeat requests to
/// the same host reuse the connection.
fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| bounded_agent(TIMEOUT_GLOBAL, TIMEOUT_CONNECT))
}

fn bounded_agent(global: Duration, connect: Duration) -> Agent {
    Agent::new_with_config(
        Agent::config_builder()
            .timeout_global(Some(global))
            .timeout_connect(Some(connect))
            .build(),
    )
}

pub fn fetch_forecast(lat: f64, lon: f64) -> Result<Weather> {
    let aqi = thread::spawn(move || fetch_air_quality(lat, lon));

    // Hold the result rather than propagating it. `?` here would return while
    // the air-quality thread was still running, detaching it — so a run of
    // early forecast failures left a pile of orphaned requests behind.
    let forecast = fetch_daily(lat, lon);

    let report = match aqi.join() {
        Ok(Ok(report)) => report,
        _ => AirQualityReport::default(),
    };

    let mut weather = forecast?;
    weather.air_quality = report.current;
    for day in &mut weather.daily {
        day.aqi = report.daily_max.get(&day.date).copied();
    }

    Ok(weather)
}

fn fetch_daily(lat: f64, lon: f64) -> Result<Weather> {
    let mut response = agent().get("https://api.open-meteo.com/v1/forecast")
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query(
            "current",
            "temperature_2m,apparent_temperature,weather_code,wind_speed_10m",
        )
        .query(
            "daily",
            "weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,wind_speed_10m_max,uv_index_max,sunrise,sunset,apparent_temperature_max,apparent_temperature_min,precipitation_sum,precipitation_hours,wind_gusts_10m_max,wind_direction_10m_dominant,daylight_duration",
        )
        .query("timezone", "auto")
        .query("forecast_days", "8")
        .query("past_days", "14")
        .call()?;

    let dto: ForecastDto = response.body_mut().read_json()?;
    Ok(dto.into())
}

pub fn search_locations(query: &str) -> Result<Vec<Location>> {
    let mut response = agent()
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query("name", query)
        .query("count", "5")
        .query("language", "en")
        .query("format", "json")
        .call()?;

    let dto: GeocodeDto = response.body_mut().read_json()?;

    Ok(dto
        .results
        .into_iter()
        .filter_map(GeocodeResultDto::into_location)
        .collect())
}

pub fn fetch_air_quality(lat: f64, lon: f64) -> Result<AirQualityReport> {
    // The window matches the forecast request so nearly every day the user can
    // browse to carries a figure. Coverage runs out a couple of days short of
    // the forecast horizon, which the UI shows as absence rather than zero.
    let mut response = agent()
        .get("https://air-quality-api.open-meteo.com/v1/air-quality")
        .query("latitude", lat.to_string())
        .query("longitude", lon.to_string())
        .query("current", "us_aqi")
        .query("hourly", "us_aqi")
        .query("timezone", "auto")
        .query("past_days", "14")
        .query("forecast_days", "7")
        .query("domains", "cams_global")
        .call()?;

    let dto: AqiDto = response.body_mut().read_json()?;

    Ok(dto.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::net::TcpListener;
    use std::time::Instant;

    /// The agent every request shares must actually carry the bounds, not just
    /// have constants declared near it. With ureq's defaults both of these are
    /// `None`, which is the whole finding.
    #[test]
    fn the_shared_agent_is_bounded_end_to_end() {
        let timeouts = agent().config().timeouts();

        assert_eq!(timeouts.global, Some(TIMEOUT_GLOBAL));
        assert_eq!(timeouts.connect, Some(TIMEOUT_CONNECT));
        assert!(
            TIMEOUT_CONNECT < TIMEOUT_GLOBAL,
            "connect must fit inside the end-to-end budget"
        );
    }

    /// A server that completes the handshake, reads the request and then says
    /// nothing at all. This is the case that used to block the sole worker
    /// thread forever while the UI sat on "Loading…" with no way to recover.
    /// Run against a short-bounded agent so the test costs a moment, not 15s.
    #[test]
    fn a_silent_server_times_out_instead_of_blocking_forever() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");

        thread::spawn(move || {
            // Accept, drain the request, and hold the socket open without ever
            // writing a response. Dropping it would give a clean EOF instead.
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut scratch = [0u8; 1024];
                let _ = stream.read(&mut scratch);
                held.push(stream);
            }
        });

        let agent = bounded_agent(Duration::from_millis(400), Duration::from_millis(200));
        let started = Instant::now();
        let result = agent.get(format!("http://{addr}/")).call();
        let waited = started.elapsed();

        assert!(result.is_err(), "a silent server must not read as success");
        assert!(
            waited < Duration::from_secs(5),
            "gave up after {waited:?}, which is not a bound"
        );
    }

    /// Nothing is listening, so the connect bound is what has to fire.
    #[test]
    fn an_unreachable_port_fails_rather_than_waiting() {
        // Bind then drop, so the port is almost certainly free and unserved.
        let addr = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener.local_addr().expect("local addr")
        };

        let agent = bounded_agent(Duration::from_millis(400), Duration::from_millis(200));
        let started = Instant::now();
        let result = agent.get(format!("http://{addr}/")).call();

        assert!(result.is_err(), "nothing is listening on {addr}");
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn real_fetch_carries_per_day_air_quality() {
        let weather = fetch_forecast(39.41427, -77.41054).expect("fetch");
        let covered = weather.daily.iter().filter(|d| d.aqi.is_some()).count();

        println!(
            "current AQI: {:?}",
            weather.air_quality.as_ref().map(|a| a.us_aqi)
        );
        println!("days: {}  with AQI: {covered}", weather.daily.len());
        for d in &weather.daily {
            println!("  {}  high {:>5.1}C  aqi {:?}", d.date, d.high_c, d.aqi);
        }
        assert!(
            covered > 15,
            "expected most of the window covered, got {covered}"
        );
    }
}
