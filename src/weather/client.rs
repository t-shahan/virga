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

/// The hosts this client talks to.
///
/// A seam, not a feature: with the URLs hardcoded the only reachable test was
/// one that never got a reply, so every *answered* failure — a 5xx, a body that
/// is not JSON, a captive portal returning its login page with a cheerful 200 —
/// was reasoned about rather than exercised. Pointing these at a loopback
/// server makes each of them a test.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub forecast: String,
    pub geocode: String,
    pub air_quality: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            forecast: "https://api.open-meteo.com/v1/forecast".to_string(),
            geocode: "https://geocoding-api.open-meteo.com/v1/search".to_string(),
            air_quality: "https://air-quality-api.open-meteo.com/v1/air-quality".to_string(),
        }
    }
}

pub fn fetch_forecast(lat: f64, lon: f64) -> Result<Weather> {
    fetch_forecast_with(agent(), &Endpoints::default(), lat, lon)
}

fn fetch_forecast_with(
    agent: &Agent,
    endpoints: &Endpoints,
    lat: f64,
    lon: f64,
) -> Result<Weather> {
    // A scoped thread rather than a detached one. The rule this encodes was
    // already here in prose: `?` on the forecast while air quality was still
    // running used to detach it, so a run of early failures left a pile of
    // orphaned requests behind. `scope` will not return until the child has,
    // so the borrow checker now enforces what the comment used to ask for.
    thread::scope(|scope| {
        let aqi = scope.spawn(|| fetch_air_quality_with(agent, endpoints, lat, lon));
        let forecast = fetch_daily_with(agent, endpoints, lat, lon);

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
    })
}

fn fetch_daily_with(agent: &Agent, endpoints: &Endpoints, lat: f64, lon: f64) -> Result<Weather> {
    let mut response = agent
        .get(&endpoints.forecast)
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
        // Rides the existing request rather than taking its own. ~15 KB more on
        // an already-warm pooled connection beats a second round trip with its
        // own loading state and its own failure mode.
        .query(
            "hourly",
            "precipitation,precipitation_probability,snowfall,weather_code,temperature_2m",
        )
        .query("timezone", "auto")
        .query("forecast_days", "8")
        .query("past_days", "14")
        .call()?;

    let dto: ForecastDto = response.body_mut().read_json()?;
    Ok(dto.into())
}

pub fn search_locations(query: &str) -> Result<Vec<Location>> {
    search_locations_with(agent(), &Endpoints::default(), query)
}

fn search_locations_with(
    agent: &Agent,
    endpoints: &Endpoints,
    query: &str,
) -> Result<Vec<Location>> {
    let mut response = agent
        .get(&endpoints.geocode)
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

/// Air quality is only ever fetched as part of a forecast — it shares that
/// request's coordinates and window — so it takes the endpoints from its
/// caller rather than reaching for the defaults itself.
fn fetch_air_quality_with(
    agent: &Agent,
    endpoints: &Endpoints,
    lat: f64,
    lon: f64,
) -> Result<AirQualityReport> {
    // The window matches the forecast request so nearly every day the user can
    // browse to carries a figure. Coverage runs out a couple of days short of
    // the forecast horizon, which the UI shows as absence rather than zero.
    let mut response = agent
        .get(&endpoints.air_quality)
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
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Instant;

    /// A loopback server that answers every request with the same canned
    /// response, and `Endpoints` pointing all three URLs at it.
    ///
    /// The failure that matters is not the network going away — that is the
    /// timeout case, already covered. It is a server that answers *promptly*
    /// and wrongly: an error status, a body that is not JSON, or the hotel
    /// wifi's login page delivered with a cheerful 200.
    fn serving(status: &str, content_type: &str, body: &str) -> Endpoints {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                // Drain enough of the request that the client is not blocked
                // writing while we are blocked replying.
                let mut scratch = [0u8; 4096];
                let _ = stream.read(&mut scratch);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        let base = format!("http://{addr}/");
        Endpoints {
            forecast: base.clone(),
            geocode: base.clone(),
            air_quality: base,
        }
    }

    /// Short bounds: these servers answer at once, so a test that hangs is a
    /// bug in the test, and should say so in a moment rather than in 15s.
    fn test_agent() -> Agent {
        bounded_agent(Duration::from_secs(5), Duration::from_secs(2))
    }

    const CAPTIVE_PORTAL: &str =
        "<!doctype html><html><body><h1>Sign in to WiFi</h1></body></html>";

    #[test]
    fn a_server_error_is_not_mistaken_for_a_forecast() {
        let endpoints = serving("500 Internal Server Error", "text/plain", "upstream down");
        let result = fetch_forecast_with(&test_agent(), &endpoints, 39.414_27, -77.410_54);

        assert!(result.is_err(), "a 500 must not read as weather");
    }

    #[test]
    fn a_captive_portal_answering_200_is_not_read_as_weather() {
        let endpoints = serving("200 OK", "text/html", CAPTIVE_PORTAL);
        let result = fetch_forecast_with(&test_agent(), &endpoints, 39.414_27, -77.410_54);

        assert!(
            result.is_err(),
            "a login page with a 200 must fail, not render as a blank forecast"
        );
    }

    /// Every *measurement* is optional so one null cannot cost the series — but
    /// the rule has to stop at the top level. If `current` and `daily` were
    /// optional too, this body would parse into an empty screen with no error.
    #[test]
    fn a_forecast_missing_its_required_blocks_fails_rather_than_emptying_the_screen() {
        let endpoints = serving("200 OK", "application/json", "{}");
        let result = fetch_forecast_with(&test_agent(), &endpoints, 39.414_27, -77.410_54);

        assert!(result.is_err(), "an empty object is not a forecast");
    }

    #[test]
    fn a_truncated_body_fails_rather_than_parsing_to_nothing() {
        let endpoints = serving("200 OK", "application/json", "{\"current\":{\"tem");
        let result = fetch_forecast_with(&test_agent(), &endpoints, 39.414_27, -77.410_54);

        assert!(result.is_err(), "half a document is not a forecast");
    }

    /// The degradation rule, exercised rather than asserted: air quality is
    /// supplementary, so losing it costs the reading, never the forecast.
    #[test]
    fn air_quality_failing_does_not_cost_the_forecast() {
        let good = serving(
            "200 OK",
            "application/json",
            include_str!("../../tests/fixtures/forecast.json"),
        );
        let broken = serving("503 Service Unavailable", "text/plain", "no aqi today");
        let endpoints = Endpoints {
            air_quality: broken.air_quality,
            ..good
        };

        let weather = fetch_forecast_with(&test_agent(), &endpoints, 39.414_27, -77.410_54)
            .expect("a dead air-quality endpoint must not fail the forecast");

        assert!(!weather.daily.is_empty(), "the forecast still arrived");
        assert!(
            weather.air_quality.is_none(),
            "no reading is the right answer, not a fabricated one"
        );
        assert!(
            weather.daily.iter().all(|d| d.aqi.is_none()),
            "and no day should carry one either"
        );
    }

    #[test]
    fn a_search_error_status_is_not_an_empty_result_list() {
        let endpoints = serving("404 Not Found", "text/plain", "nope");
        let result = search_locations_with(&test_agent(), &endpoints, "reykjavik");

        assert!(
            result.is_err(),
            "a 404 must not be indistinguishable from 'no cities matched'"
        );
    }

    #[test]
    fn a_captive_portal_on_search_is_not_an_empty_result_list() {
        let endpoints = serving("200 OK", "text/html", CAPTIVE_PORTAL);
        let result = search_locations_with(&test_agent(), &endpoints, "reykjavik");

        assert!(result.is_err(), "a login page is not a list of cities");
    }

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

    /// The hourly block rides the forecast request, so a schema change or a
    /// dropped parameter would silently empty the precipitation screen rather
    /// than fail anything. An operational smoke test, not a contract.
    #[test]
    #[ignore]
    fn real_fetch_carries_a_populated_hourly_series() {
        let weather = fetch_forecast(39.41427, -77.41054).expect("fetch");
        let forward = weather.forecast_hours();

        println!(
            "hourly {} · now_hour {} · forward {}",
            weather.hourly.len(),
            weather.now_hour,
            forward.len()
        );
        println!("first forward hour: {:?}", forward.first().map(|h| &h.time));

        assert!(!weather.hourly.is_empty(), "no hourly block arrived");
        assert!(
            weather.now_hour > 0,
            "the request carries past_days, so now cannot be the first hour"
        );
        assert!(
            forward.len() > 24,
            "expected more than a day ahead, got {}",
            forward.len()
        );
        assert!(
            forward.iter().all(|h| h.chance.is_some()),
            "probability was populated for every hour when this was written"
        );
        assert!(
            forward.iter().any(|h| h.precip_mm.is_some()),
            "amount should be present even when it is zero"
        );
    }
}
