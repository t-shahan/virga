use crate::weather::dto::AqiDto;
use crate::weather::dto::ForecastDto;
use crate::weather::dto::GeoIpDto;
use crate::weather::dto::GeocodeDto;
use crate::weather::dto::GeocodeResultDto;
use crate::weather::model::AirQualityReport;
use crate::weather::model::Location;
use crate::weather::model::Weather;
use anyhow::{Context, Result};
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
const HOURLY_FIELDS: &str = "precipitation,precipitation_probability,snowfall,weather_code,temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,wind_gusts_10m,wind_direction_10m";

/// Detection sits in front of the first frame of weather, so its budget is
/// tighter than the forecast's. A provider having a bad day should cost a
/// moment and a fallback, not fifteen seconds of "locating...".
const TIMEOUT_GEOIP_GLOBAL: Duration = Duration::from_secs(5);
const TIMEOUT_GEOIP_CONNECT: Duration = Duration::from_secs(3);

/// One shared agent for the whole process. `ureq::get()` builds a fresh Agent
/// per call, and a fresh Agent means a fresh connection pool — so every request
/// re-paid a full TCP + TLS handshake. Measured against Open-Meteo that was
/// ~360ms of the ~480ms round trip. Sharing one agent lets repeat requests to
/// the same host reuse the connection.
fn agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| bounded_agent(TIMEOUT_GLOBAL, TIMEOUT_CONNECT))
}

/// Its own agent rather than the shared one: this is a different host with a
/// different budget, and pooling a connection to it would gain nothing — the
/// app talks to it once, at launch, and never again.
fn geoip_agent() -> &'static Agent {
    static AGENT: OnceLock<Agent> = OnceLock::new();
    AGENT.get_or_init(|| bounded_agent(TIMEOUT_GEOIP_GLOBAL, TIMEOUT_GEOIP_CONNECT))
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
    /// The one host here that is not Open-Meteo. HTTPS and no API key, because
    /// the request's only identifying content is the connection's own source
    /// address and it should not cross the network in the clear.
    pub geoip: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            forecast: "https://api.open-meteo.com/v1/forecast".to_string(),
            geocode: "https://geocoding-api.open-meteo.com/v1/search".to_string(),
            air_quality: "https://air-quality-api.open-meteo.com/v1/air-quality".to_string(),
            geoip: "https://ipapi.co/json/".to_string(),
        }
    }
}

/// Where the caller is, as far as their IP address gives them away.
pub fn detect_location() -> Result<Location> {
    detect_location_with(geoip_agent(), &Endpoints::default())
}

fn detect_location_with(agent: &Agent, endpoints: &Endpoints) -> Result<Location> {
    let mut response = agent.get(&endpoints.geoip).call()?;
    let dto: GeoIpDto = response.body_mut().read_json()?;

    dto.into_location()
        .context("the location service did not name a place")
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
        .query("hourly", HOURLY_FIELDS)
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
    use std::sync::{Arc, Mutex};

    #[test]
    fn hourly_request_names_every_weathergram_field() {
        assert_eq!(
            HOURLY_FIELDS.split(',').collect::<Vec<_>>(),
            vec![
                "precipitation",
                "precipitation_probability",
                "snowfall",
                "weather_code",
                "temperature_2m",
                "apparent_temperature",
                "relative_humidity_2m",
                "wind_speed_10m",
                "wind_gusts_10m",
                "wind_direction_10m",
            ]
        );
    }

    /// The forecast's window and zone are what the rest of the app is built
    /// on: two weeks of history for the daily chart, eight days ahead, and
    /// local time so the series lines up with the clock on the wall. None of
    /// it was asserted before; the request was read and thrown away.
    #[test]
    fn the_forecast_request_asks_for_the_window_the_app_is_built_on() {
        let (endpoints, log) = serving_recorded(
            "200 OK",
            "application/json",
            include_str!("../../tests/fixtures/forecast.json"),
        );
        fetch_forecast_with(&test_agent(), &endpoints, 39.414_27, -77.410_54).expect("a forecast");

        let forecast = request_with(&log, "forecast_days", "8");
        assert_eq!(value_of(&forecast, "past_days"), Some("14"));
        assert_eq!(value_of(&forecast, "timezone"), Some("auto"));
        assert_eq!(value_of(&forecast, "latitude"), Some("39.41427"));
        assert_eq!(value_of(&forecast, "longitude"), Some("-77.41054"));
        assert_eq!(
            value_of(&forecast, "hourly"),
            Some(HOURLY_FIELDS),
            "the hourly fields the screens read must be the ones asked for"
        );
        for field in [
            "weather_code",
            "temperature_2m_max",
            "sunrise",
            "daylight_duration",
        ] {
            assert!(
                value_of(&forecast, "daily")
                    .is_some_and(|daily| daily.split(',').any(|f| f == field)),
                "the daily request lost {field}: {forecast:?}"
            );
        }
    }

    /// The air-quality request shares the forecast's coordinates and window,
    /// and names its model domain outright rather than taking the provider's
    /// default. The per-day figures line up with the daily chart only while
    /// the two windows agree, which nothing checked before this.
    #[test]
    fn the_air_quality_request_matches_the_forecast_window_on_the_global_domain() {
        let (endpoints, log) = serving_recorded(
            "200 OK",
            "application/json",
            include_str!("../../tests/fixtures/forecast.json"),
        );
        fetch_forecast_with(&test_agent(), &endpoints, 39.414_27, -77.410_54).expect("a forecast");

        let aqi = request_with(&log, "domains", "cams_global");
        assert_eq!(value_of(&aqi, "current"), Some("us_aqi"));
        assert_eq!(value_of(&aqi, "hourly"), Some("us_aqi"));
        assert_eq!(value_of(&aqi, "timezone"), Some("auto"));
        assert_eq!(value_of(&aqi, "past_days"), Some("14"));
        assert_eq!(value_of(&aqi, "forecast_days"), Some("7"));
        assert_eq!(value_of(&aqi, "latitude"), Some("39.41427"));
        assert_eq!(value_of(&aqi, "longitude"), Some("-77.41054"));
    }

    /// Five results is what the search screen has rows for, and the query
    /// goes across as typed: a space must not split it into two parameters.
    #[test]
    fn the_search_request_carries_the_query_and_asks_for_five_results() {
        let (endpoints, log) = serving_recorded("200 OK", "application/json", r#"{"results":[]}"#);
        search_locations_with(&test_agent(), &endpoints, "new york").expect("a result list");

        let search = request_with(&log, "count", "5");
        assert_eq!(value_of(&search, "name"), Some("new york"));
        assert_eq!(value_of(&search, "language"), Some("en"));
        assert_eq!(value_of(&search, "format"), Some("json"));
    }

    /// Detection sends nothing about the caller beyond the connection itself.
    #[test]
    fn the_detection_request_carries_no_query_at_all() {
        let (endpoints, log) = serving_recorded("200 OK", "application/json", DETECTED);
        detect_location_with(&test_agent(), &endpoints).expect("a detection");

        let lines = log.lock().expect("request log").clone();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("GET / HTTP/1.1"), "{:?}", lines[0]);
    }

    /// A loopback server that answers every request with the same canned
    /// response, and `Endpoints` pointing all three URLs at it.
    ///
    /// The failure that matters is not the network going away — that is the
    /// timeout case, already covered. It is a server that answers *promptly*
    /// and wrongly: an error status, a body that is not JSON, or the hotel
    /// wifi's login page delivered with a cheerful 200.
    fn serving(status: &str, content_type: &str, body: &str) -> Endpoints {
        serving_recorded(status, content_type, body).0
    }

    /// The request lines the loopback server has answered, in arrival order.
    /// The forecast and its air-quality request run on two threads, so a
    /// test picks its line out by content rather than by position.
    type RequestLog = Arc<Mutex<Vec<String>>>;

    /// `serving`, plus a log of every request line it answered. What the
    /// client *sends* was unverified until this: the server read the request
    /// to unblock the client and threw it away, so a dropped `past_days` or a
    /// renamed field would have failed nothing here.
    fn serving_recorded(status: &str, content_type: &str, body: &str) -> (Endpoints, RequestLog) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let log: RequestLog = Arc::default();
        let recorder = Arc::clone(&log);

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                // Read the whole header block, not just the first chunk: it
                // both unblocks the client, which may be waiting to finish
                // writing before it reads, and guarantees the request line
                // was seen whole before it is logged.
                let head = read_request_head(&mut stream);
                if let Some(line) = head.lines().next()
                    && let Ok(mut log) = recorder.lock()
                {
                    log.push(line.to_string());
                }
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        let base = format!("http://{addr}/");
        let endpoints = Endpoints {
            forecast: base.clone(),
            geocode: base.clone(),
            air_quality: base.clone(),
            geoip: base,
        };
        (endpoints, log)
    }

    /// Everything up to the blank line that ends the headers, or as much as
    /// arrived before the client stopped sending. Bounded so a client that
    /// never sends the blank line cannot grow the buffer without limit.
    fn read_request_head(stream: &mut impl Read) -> String {
        let mut head = Vec::new();
        let mut chunk = [0u8; 1024];
        while head.len() < 64 * 1024 {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => head.extend_from_slice(&chunk[..n]),
            }
            if head.windows(4).any(|w| w == b"\r\n\r\n") {
                break;
            }
        }
        String::from_utf8_lossy(&head).into_owned()
    }

    /// The query pairs of a logged request line, percent-decoded, so a test
    /// can assert on the field list it reads rather than on `%2C`.
    fn query_pairs(request_line: &str) -> Vec<(String, String)> {
        let target = request_line.split(' ').nth(1).unwrap_or_default();
        let Some((_, query)) = target.split_once('?') else {
            return Vec::new();
        };
        query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
                (percent_decode(key), percent_decode(value))
            })
            .collect()
    }

    fn percent_decode(encoded: &str) -> String {
        let bytes = encoded.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            match bytes[i] {
                b'+' => out.push(b' '),
                b'%' if i + 2 < bytes.len() => {
                    let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                    match u8::from_str_radix(hex, 16) {
                        Ok(byte) => {
                            out.push(byte);
                            i += 2;
                        }
                        Err(_) => out.push(b'%'),
                    }
                }
                byte => out.push(byte),
            }
            i += 1;
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    /// The one logged request whose query carries `key=value`, as decoded
    /// pairs. Panics on none or several: each test expects exactly one
    /// request of each kind, and a duplicate would be its own finding.
    fn request_with(log: &RequestLog, key: &str, value: &str) -> Vec<(String, String)> {
        let lines = log.lock().expect("request log").clone();
        let mut matching = lines
            .iter()
            .map(|line| query_pairs(line))
            .filter(|pairs| pairs.iter().any(|(k, v)| k == key && v == value));
        let found = matching
            .next()
            .unwrap_or_else(|| panic!("no request carried {key}={value}: {lines:?}"));
        assert!(
            matching.next().is_none(),
            "more than one request carried {key}={value}: {lines:?}"
        );
        found
    }

    fn value_of<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
        pairs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
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

    const DETECTED: &str = r#"{"city":"Reykjavík","region":"Capital Region",
        "country_name":"Iceland","latitude":64.14659,"longitude":-21.94223}"#;

    #[test]
    fn a_detection_becomes_a_labelled_location() {
        let endpoints = serving("200 OK", "application/json", DETECTED);
        let found = detect_location_with(&test_agent(), &endpoints).expect("a detection");

        assert_eq!(found.label(), "Reykjavík, Capital Region, Iceland");
        assert_eq!(found.lat, 64.14659);
        assert_eq!(found.lon, -21.94223);
    }

    /// The provider reports rate limiting and reserved addresses as a *200*
    /// carrying an error object. Read loosely that body has no coordinates, and
    /// "no coordinates" read loosely is `0, 0` — so what this guards against is
    /// launching in the Gulf of Guinea rather than launching with an error.
    #[test]
    fn an_error_object_with_a_200_is_not_a_location() {
        let endpoints = serving(
            "200 OK",
            "application/json",
            r#"{"error":true,"reason":"RateLimited","latitude":0,"longitude":0}"#,
        );

        assert!(detect_location_with(&test_agent(), &endpoints).is_err());
    }

    #[test]
    fn unusable_detections_are_failures_rather_than_places() {
        for (name, status, content_type, body) in [
            (
                "rate-limited",
                "429 Too Many Requests",
                "text/plain",
                "slow down",
            ),
            ("captive-portal", "200 OK", "text/html", CAPTIVE_PORTAL),
            (
                "no-coordinates",
                "200 OK",
                "application/json",
                r#"{"city":"Nowhere"}"#,
            ),
            (
                "null-island",
                "200 OK",
                "application/json",
                r#"{"city":"Nowhere","latitude":0,"longitude":0}"#,
            ),
            (
                "nameless",
                "200 OK",
                "application/json",
                r#"{"latitude":64.14659,"longitude":-21.94223}"#,
            ),
            (
                "out-of-range",
                "200 OK",
                "application/json",
                r#"{"city":"Nowhere","latitude":91.0,"longitude":0.0}"#,
            ),
        ] {
            let endpoints = serving(status, content_type, body);
            assert!(
                detect_location_with(&test_agent(), &endpoints).is_err(),
                "{name} was accepted as a location"
            );
        }
    }

    /// Partial answers are the common case — a mobile network resolves to a
    /// country and no city more often than it resolves to nothing — and a place
    /// with a coarse name still beats New York.
    #[test]
    fn a_partial_detection_still_names_somewhere() {
        for (body, expected) in [
            (
                r#"{"region":"Capital Region","country_name":"Iceland","latitude":64.1,"longitude":-21.9}"#,
                "Capital Region, Iceland",
            ),
            (
                r#"{"country_name":"Iceland","latitude":64.1,"longitude":-21.9}"#,
                "Iceland",
            ),
            (
                r#"{"city":"Reykjavík","latitude":64.1,"longitude":-21.9}"#,
                "Reykjavík",
            ),
        ] {
            let endpoints = serving("200 OK", "application/json", body);
            let found = detect_location_with(&test_agent(), &endpoints).expect("a detection");

            assert_eq!(found.label(), expected);
        }
    }

    /// The lookup runs before the first frame of weather, so its budget has to
    /// be the tighter of the two — not merely declared nearby.
    #[test]
    fn the_detection_agent_is_bounded_more_tightly_than_the_weather_agent() {
        let timeouts = geoip_agent().config().timeouts();

        assert_eq!(timeouts.global, Some(TIMEOUT_GEOIP_GLOBAL));
        assert_eq!(timeouts.connect, Some(TIMEOUT_GEOIP_CONNECT));
        assert!(TIMEOUT_GEOIP_GLOBAL < TIMEOUT_GLOBAL);
        assert!(TIMEOUT_GEOIP_CONNECT < TIMEOUT_GEOIP_GLOBAL);
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
    ///
    /// Only the error is asserted. The bound is what produces it, since the
    /// server never answers, so measuring the wait as well proved nothing the
    /// error had not and would fail on a starved runner.
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
        let result = agent.get(format!("http://{addr}/")).call();

        assert!(result.is_err(), "a silent server must not read as success");
    }

    /// A port that is bound but never accepted from. The earlier form bound a
    /// port, dropped it, and assumed nothing would claim it before the call;
    /// but every `serving()` in this process binds an ephemeral port on the
    /// same loopback, so another test could take it and answer, and the
    /// assertion here would fail for no fault in the client. A listener that
    /// is held and never accepted from is the same port for the whole test:
    /// the kernel completes the handshake into the backlog and the request
    /// then waits on nobody, so the end-to-end bound is what fires.
    #[test]
    fn a_port_nobody_accepts_from_fails_rather_than_waiting() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");

        let agent = bounded_agent(Duration::from_millis(400), Duration::from_millis(200));
        let result = agent.get(format!("http://{addr}/")).call();

        assert!(result.is_err(), "nobody accepts on {addr}");
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

    /// The detection provider is not Open-Meteo and is not covered by anything
    /// in CI, so a field rename at their end would show up as everyone quietly
    /// launching in New York. An operational smoke test, not a contract — it
    /// prints what it resolved so a human can see whether it is plausible.
    #[test]
    #[ignore]
    fn real_detection_names_somewhere_plausible() {
        let found = detect_location().expect("detect");

        println!(
            "detected: {} at {}, {}",
            found.label(),
            found.lat,
            found.lon
        );

        assert!(!found.label().trim().is_empty());
        assert!((-90.0..=90.0).contains(&found.lat));
        assert!((-180.0..=180.0).contains(&found.lon));
        assert!(
            found.lat != 0.0 || found.lon != 0.0,
            "Null Island is not a plausible answer"
        );
    }

    /// The hourly block rides the forecast request, so a schema change or a
    /// dropped parameter would silently empty the hourly screen rather
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
        assert!(
            forward.iter().any(|h| h.feels_like_c.is_some()),
            "apparent temperature should be present"
        );
        assert!(
            forward.iter().any(|h| h.humidity_pct.is_some()),
            "humidity should be present"
        );
        assert!(
            forward.iter().any(|h| h.wind_kph.is_some()),
            "wind speed should be present"
        );
        assert!(
            forward.iter().any(|h| h.gust_kph.is_some()),
            "wind gusts should be present"
        );
        assert!(
            forward.iter().any(|h| h.wind_dir_deg.is_some()),
            "wind direction should be present"
        );
    }
}
