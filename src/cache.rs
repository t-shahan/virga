//! The last forecast, kept on disk so the next launch can open on it.
//!
//! A launch used to open on a spinner and wait for Open-Meteo, half a second
//! warm and up to two cold (#54). The forecast from the previous launch is
//! still a forecast, so it is painted first, labelled with its age, and
//! replaced in place when the fresh one lands.
//!
//! Kept apart from `state.json`, in its own file with its own version: the
//! remembered city must never be at the mercy of a change to the weather
//! model, and a cache is the one document that may be thrown away freely.

use crate::app::{ActiveLocation, CachedWeather};
use crate::state;
use crate::weather::model::Weather;
use anyhow::{Context, Result};
use chrono::{DateTime, Local, NaiveDateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VERSION: u8 = 1;
const FILE: &str = "forecast.json";
/// The oldest forecast worth opening on. Beyond a day the hero temperature
/// is a different day's, and the hourly series has less than a week left.
/// The age is shown beside the forecast whatever it is; this only decides
/// whether it is shown at all.
const MAX_AGE: TimeDelta = TimeDelta::hours(24);
/// The stamp Open-Meteo puts on `current.time`, local to the location.
const STAMP: &str = "%Y-%m-%dT%H:%M";

/// The body behind the envelope. The version is read from the envelope
/// alone, before this is attempted, so a newer body never reads as corrupt.
#[derive(Deserialize)]
struct Document {
    location: ActiveLocation,
    /// UTC seconds. The age bound and the wording of the label come from it.
    fetched_at: i64,
    weather: Weather,
}

/// The one claim every version makes. See `state::surviving_document`.
#[derive(Deserialize)]
struct VersionEnvelope {
    version: u8,
}

pub fn path_beside(state: &Path) -> PathBuf {
    state.with_file_name(FILE)
}

/// The cache, if there is one this launch may open on.
///
/// `None`, silently, when there is no file, when it describes somewhere
/// other than `expected`, when it is older than `MAX_AGE`, or when its
/// series does not reach the current hour. A file that is there and cannot
/// be read is an error for the caller to report; it is never a reason not to
/// start.
///
/// `now` is passed in so a test never reads the clock.
pub fn load(
    path: &Path,
    expected: &ActiveLocation,
    now: DateTime<Local>,
) -> Result<Option<CachedWeather>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let envelope: VersionEnvelope =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    // A newer binary's cache is not corrupt, and not ours to read. Opening
    // without it costs one spinner.
    if envelope.version > VERSION {
        return Ok(None);
    }
    let document: Document =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;

    if !document.location.same_place(expected) {
        return Ok(None);
    }
    let Some(fetched) = DateTime::<Utc>::from_timestamp(document.fetched_at, 0) else {
        return Ok(None);
    };
    let age = now.with_timezone(&Utc) - fetched;
    if age < TimeDelta::zero() || age > MAX_AGE {
        return Ok(None);
    }

    // Local time at the city now is the reading's own stamp plus what has
    // elapsed since; the user's clock never enters it, which is what keeps a
    // forecast for another timezone pointing at the right hour.
    let mut weather = document.weather;
    let Some(observed) = weather
        .current
        .observed
        .as_deref()
        .and_then(|stamp| NaiveDateTime::parse_from_str(stamp, STAMP).ok())
    else {
        return Ok(None);
    };
    let stamp = (observed + age).format(STAMP).to_string();
    if !weather.relocate(&stamp) {
        return Ok(None);
    }

    Ok(Some(CachedWeather {
        weather,
        as_of: as_of(fetched.with_timezone(&Local), now),
    }))
}

/// "17:52" for a fetch earlier today, "yesterday 22:14" for one before
/// midnight. Under `MAX_AGE` those are the only two cases.
fn as_of(fetched: DateTime<Local>, now: DateTime<Local>) -> String {
    let time = fetched.format("%H:%M");
    if fetched.date_naive() == now.date_naive() {
        time.to_string()
    } else {
        format!("yesterday {time}")
    }
}

/// The document to write for a forecast just fetched. Serialized apart from
/// `write` so the worker can hand the forecast to the app first and pay for
/// the file afterwards.
pub fn encode(
    location: &ActiveLocation,
    weather: &Weather,
    fetched_at: DateTime<Utc>,
) -> Result<Vec<u8>> {
    // `Weather` is not `Clone`, and the model should not grow one for a
    // single caller, so the document is serialized over borrows.
    let mut bytes = serde_json::to_vec(&Borrowed {
        version: VERSION,
        location,
        fetched_at: fetched_at.timestamp(),
        weather,
    })?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// `Document` over borrows, for writing.
#[derive(Serialize)]
struct Borrowed<'a> {
    version: u8,
    location: &'a ActiveLocation,
    fetched_at: i64,
    weather: &'a Weather,
}

/// Replace the cache with `bytes`, atomically and under the same kind of
/// lock as the state file. Refuses to replace a cache written by a newer
/// virga, as `state` does: what a later format holds is not ours to lose.
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    let _held = state::exclusive(path)?;
    if let Ok(existing) = std::fs::read(path)
        && let Ok(envelope) = serde_json::from_slice::<VersionEnvelope>(&existing)
    {
        anyhow::ensure!(
            envelope.version <= VERSION,
            "the forecast cache is version {}, written by a newer virga; refusing to overwrite it",
            envelope.version
        );
    }
    let parent = path.parent().context("cache path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary cache in {}", parent.display()))?;
    use std::io::Write as _;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn frederick() -> ActiveLocation {
        ActiveLocation {
            label: "Frederick, Maryland, United States".to_string(),
            lat: 39.414_27,
            lon: -77.410_54,
        }
    }

    /// The fixture's hours open at 2026-08-01T00:00; its reading is stamped
    /// a day in, at the fixture's own `now_hour`.
    fn forecast() -> Weather {
        let mut weather = Weather::fixture(9, 1);
        weather.current.observed = Some("2026-08-02T00:00".to_string());
        weather
    }

    fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap()
    }

    fn written(dir: &Path, fetched: DateTime<Local>) -> PathBuf {
        let path = dir.join(FILE);
        let bytes = encode(&frederick(), &forecast(), fetched.with_timezone(&Utc)).unwrap();
        write(&path, &bytes).unwrap();
        path
    }

    #[test]
    fn a_forecast_comes_back_relocated_to_the_current_hour() {
        let dir = tempfile::tempdir().unwrap();
        let fetched = local(2026, 8, 31, 17, 52);
        let path = written(dir.path(), fetched);

        let cached = load(&path, &frederick(), local(2026, 8, 31, 21, 5))
            .unwrap()
            .expect("a three hour old forecast opens the app");
        assert_eq!(cached.as_of, "17:52");
        // 00:00 on the reading's clock plus 3 h 13 m is the 03:00 entry.
        assert_eq!(cached.weather.now_hour, 27);
        assert_eq!(cached.weather.today_index, 1);
        assert_eq!(cached.weather.hourly.len(), forecast().hourly.len());
    }

    #[test]
    fn a_fetch_before_midnight_says_yesterday() {
        let dir = tempfile::tempdir().unwrap();
        let path = written(dir.path(), local(2026, 8, 30, 22, 14));

        let cached = load(&path, &frederick(), local(2026, 8, 31, 1, 0))
            .unwrap()
            .unwrap();
        assert_eq!(cached.as_of, "yesterday 22:14");
    }

    #[test]
    fn a_day_old_forecast_is_not_opened_on() {
        let dir = tempfile::tempdir().unwrap();
        let fetched = local(2026, 8, 30, 12, 0);
        let path = written(dir.path(), fetched);

        let just_inside = fetched + TimeDelta::hours(24);
        assert!(load(&path, &frederick(), just_inside).unwrap().is_some());
        let just_outside = just_inside + TimeDelta::minutes(1);
        assert!(load(&path, &frederick(), just_outside).unwrap().is_none());
        assert!(
            load(&path, &frederick(), fetched - TimeDelta::minutes(1))
                .unwrap()
                .is_none(),
            "a clock that has gone backwards is not trusted either"
        );
    }

    #[test]
    fn somewhere_else_is_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = written(dir.path(), local(2026, 8, 31, 17, 52));
        let elsewhere = ActiveLocation {
            label: "Frederick, Maryland, United States".to_string(),
            lat: 40.0,
            lon: -77.410_54,
        };

        assert!(
            load(&path, &elsewhere, local(2026, 8, 31, 18, 0))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_missing_file_is_no_cache_and_no_complaint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        assert!(
            load(&path, &frederick(), local(2026, 8, 31, 18, 0))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_body_that_does_not_parse_is_an_error_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        for body in [
            "",
            "{",
            "{\"version\":1}",
            "{\"version\":1,\"weather\":null}",
        ] {
            std::fs::write(&path, body).unwrap();
            assert!(
                load(&path, &frederick(), local(2026, 8, 31, 18, 0)).is_err(),
                "{body:?}"
            );
        }
    }

    #[test]
    fn a_newer_cache_is_neither_read_nor_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, "{\"version\":99,\"anything\":true}").unwrap();

        assert!(
            load(&path, &frederick(), local(2026, 8, 31, 18, 0))
                .unwrap()
                .is_none()
        );
        let bytes = encode(&frederick(), &forecast(), Utc::now()).unwrap();
        let error = write(&path, &bytes).unwrap_err().to_string();
        assert!(error.contains("newer virga"), "{error}");
        assert!(std::fs::read_to_string(&path).unwrap().contains("99"));
    }

    /// A reading with no stamp cannot be relocated, and a forecast that has
    /// run out of hours must not open the app on its last one.
    #[test]
    fn a_forecast_that_cannot_be_relocated_is_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let fetched = local(2026, 8, 31, 12, 0);

        let mut unstamped = forecast();
        unstamped.current.observed = None;
        write(
            &path,
            &encode(&frederick(), &unstamped, fetched.with_timezone(&Utc)).unwrap(),
        )
        .unwrap();
        assert!(
            load(&path, &frederick(), fetched + TimeDelta::hours(1))
                .unwrap()
                .is_none()
        );

        let mut ending = forecast();
        ending.current.observed = Some("2026-08-09T22:00".to_string());
        write(
            &path,
            &encode(&frederick(), &ending, fetched.with_timezone(&Utc)).unwrap(),
        )
        .unwrap();
        assert!(
            load(&path, &frederick(), fetched + TimeDelta::hours(3))
                .unwrap()
                .is_none()
        );
    }
}
