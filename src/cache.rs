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
use chrono::{DateTime, FixedOffset, Local, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VERSION: u8 = 1;
const FILE: &str = "forecast.json";
/// The oldest forecast worth opening on. Beyond a day the hero temperature
/// is a different day's, and the hourly series has less than a week left.
/// The age is shown beside the forecast whatever it is; this only decides
/// whether it is shown at all.
pub const MAX_AGE: TimeDelta = TimeDelta::hours(24);
/// The oldest forecast `virga now` may answer from. Shorter than `MAX_AGE`
/// because the report carries no "as of" mark: whatever it prints has to
/// pass for the answer to a question asked now. The conditions line is the
/// observation at the fetch, which an hour on is the previous hour's, and an
/// hour is also the resolution of the series beside it. Not shorter, or a
/// status bar polling by the minute would be answered from disk only in
/// the minutes after a launch.
pub const REPORT_MAX_AGE: TimeDelta = TimeDelta::hours(1);
/// The shape of the series' timestamps, local to the location.
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

/// The cache, if there is one under `max_age` that describes `expected`:
/// `MAX_AGE` for a launch to open on, `REPORT_MAX_AGE` for `virga now` to
/// print.
///
/// `None`, silently, when there is no file, when it describes somewhere
/// other than `expected`, when it is older than `max_age`, or when its
/// series does not reach the current hour. A file that is there and cannot
/// be read is an error for the caller to report; it is never a reason not to
/// start.
///
/// `now` is passed in so a test never reads the clock.
pub fn load(
    path: &Path,
    expected: &ActiveLocation,
    now: DateTime<Local>,
    max_age: TimeDelta,
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
    if age < TimeDelta::zero() || age > max_age {
        return Ok(None);
    }

    // Local time at the city is UTC now shifted by the offset the response
    // itself carried. Not the reading's stamp plus the elapsed time: the
    // stamp is model data, up to a quarter hour behind the fetch, and that
    // slack pointed a launch just after midnight at the previous day. The
    // one thing this trusts across the gap is the offset, which a DST
    // transition inside the 24 h window can move — the wrong hour that
    // leaves is corrected by the fetch already on its way, where the wrong
    // day stood until `r`.
    let mut weather = document.weather;
    let Some(offset) = weather.utc_offset_secs.and_then(FixedOffset::east_opt) else {
        return Ok(None);
    };
    let stamp = now.with_timezone(&offset).format(STAMP).to_string();
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

    /// The fixture's hours run 2026-08-01T00:00 through 2026-08-09T23:00.
    /// The city is placed in the test machine's own zone — the offset is
    /// read off `at` — so every expected position below can be written as a
    /// plain local stamp whatever zone the tests run in.
    fn forecast(at: DateTime<Local>) -> Weather {
        let mut weather = Weather::fixture(9, 1);
        weather.utc_offset_secs = Some(at.offset().local_minus_utc());
        weather
    }

    fn local(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(y, mo, d, h, mi, 0).single().unwrap()
    }

    fn written(dir: &Path, fetched: DateTime<Local>) -> PathBuf {
        let path = dir.join(FILE);
        let bytes = encode(
            &frederick(),
            &forecast(fetched),
            fetched.with_timezone(&Utc),
        )
        .unwrap();
        write(&path, &bytes).unwrap();
        path
    }

    #[test]
    fn a_forecast_comes_back_relocated_to_the_current_hour() {
        let dir = tempfile::tempdir().unwrap();
        let fetched = local(2026, 8, 2, 17, 52);
        let path = written(dir.path(), fetched);

        let now = local(2026, 8, 2, 21, 5);
        let cached = load(&path, &frederick(), now, MAX_AGE)
            .unwrap()
            .expect("a three hour old forecast opens the app");
        assert_eq!(cached.as_of, "17:52");
        // 21:05 on the second day of the series is its 21:00 entry.
        assert_eq!(cached.weather.now_hour, 45);
        assert_eq!(cached.weather.today_index, 1);
        assert_eq!(cached.weather.hourly.len(), forecast(now).hourly.len());
    }

    /// The regression the observation stamp used to cause: model data can
    /// stamp the reading a quarter hour behind the fetch, so deriving the
    /// city's clock from the stamp pointed a launch just after midnight at
    /// the previous day. The positions come from the clock now, so crossing
    /// midnight between the fetch and the launch lands on the new day.
    #[test]
    fn reopening_after_midnight_lands_on_the_new_day() {
        let dir = tempfile::tempdir().unwrap();
        let path = written(dir.path(), local(2026, 8, 2, 23, 59));

        let cached = load(&path, &frederick(), local(2026, 8, 3, 0, 5), MAX_AGE)
            .unwrap()
            .expect("a six minute old forecast opens the app");
        assert_eq!(cached.weather.today_index, 2, "midnight has passed");
        assert_eq!(cached.weather.now_hour, 48);
        assert_eq!(cached.as_of, "yesterday 23:59");
    }

    /// The same rule one tier down: crossing an hour boundary moves "now"
    /// to the new hour's entry, however recent the fetch.
    #[test]
    fn reopening_after_an_hour_boundary_lands_on_the_new_hour() {
        let dir = tempfile::tempdir().unwrap();
        let path = written(dir.path(), local(2026, 8, 2, 13, 59));

        let cached = load(&path, &frederick(), local(2026, 8, 2, 14, 1), MAX_AGE)
            .unwrap()
            .expect("a two minute old forecast opens the app");
        assert_eq!(cached.weather.now_hour, 38);
        assert_eq!(cached.weather.today_index, 1);
    }

    #[test]
    fn a_fetch_before_midnight_says_yesterday() {
        let dir = tempfile::tempdir().unwrap();
        let path = written(dir.path(), local(2026, 8, 1, 22, 14));

        let cached = load(&path, &frederick(), local(2026, 8, 2, 1, 0), MAX_AGE)
            .unwrap()
            .unwrap();
        assert_eq!(cached.as_of, "yesterday 22:14");
    }

    #[test]
    fn a_day_old_forecast_is_not_opened_on() {
        let dir = tempfile::tempdir().unwrap();
        let fetched = local(2026, 8, 2, 12, 0);
        let path = written(dir.path(), fetched);

        let just_inside = fetched + TimeDelta::hours(24);
        assert!(
            load(&path, &frederick(), just_inside, MAX_AGE)
                .unwrap()
                .is_some()
        );
        let just_outside = just_inside + TimeDelta::minutes(1);
        assert!(
            load(&path, &frederick(), just_outside, MAX_AGE)
                .unwrap()
                .is_none()
        );
        assert!(
            load(
                &path,
                &frederick(),
                fetched - TimeDelta::minutes(1),
                MAX_AGE
            )
            .unwrap()
            .is_none(),
            "a clock that has gone backwards is not trusted either"
        );
    }

    /// The one-shot report's bound, an hour, is the same edge one tier down:
    /// the minute after it, the file is still a launch's cache and no longer
    /// a report's.
    #[test]
    fn a_report_is_held_to_a_tighter_bound_than_a_launch() {
        let dir = tempfile::tempdir().unwrap();
        let fetched = local(2026, 8, 2, 12, 0);
        let path = written(dir.path(), fetched);

        let just_inside = fetched + REPORT_MAX_AGE;
        assert!(
            load(&path, &frederick(), just_inside, REPORT_MAX_AGE)
                .unwrap()
                .is_some()
        );
        let just_outside = just_inside + TimeDelta::minutes(1);
        assert!(
            load(&path, &frederick(), just_outside, REPORT_MAX_AGE)
                .unwrap()
                .is_none()
        );
        assert!(
            load(&path, &frederick(), just_outside, MAX_AGE)
                .unwrap()
                .is_some(),
            "a launch still opens on it"
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
            load(&path, &elsewhere, local(2026, 8, 31, 18, 0), MAX_AGE)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_missing_file_is_no_cache_and_no_complaint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        assert!(
            load(&path, &frederick(), local(2026, 8, 31, 18, 0), MAX_AGE)
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
                load(&path, &frederick(), local(2026, 8, 31, 18, 0), MAX_AGE).is_err(),
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
            load(&path, &frederick(), local(2026, 8, 31, 18, 0), MAX_AGE)
                .unwrap()
                .is_none()
        );
        let bytes = encode(&frederick(), &forecast(Local::now()), Utc::now()).unwrap();
        let error = write(&path, &bytes).unwrap_err().to_string();
        assert!(error.contains("newer virga"), "{error}");
        assert!(std::fs::read_to_string(&path).unwrap().contains("99"));
    }

    /// A cache with no offset cannot say what time it is at the city, and a
    /// forecast that has run out of hours must not open the app on its last
    /// one — both are refused whole rather than shown at a guessed position.
    #[test]
    fn a_forecast_that_cannot_be_relocated_is_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);

        let fetched = local(2026, 8, 2, 12, 0);
        let mut offsetless = forecast(fetched);
        offsetless.utc_offset_secs = None;
        write(
            &path,
            &encode(&frederick(), &offsetless, fetched.with_timezone(&Utc)).unwrap(),
        )
        .unwrap();
        assert!(
            load(&path, &frederick(), fetched + TimeDelta::hours(1), MAX_AGE)
                .unwrap()
                .is_none()
        );

        // The series' last hour is 2026-08-09T23:00; two hours past it is
        // inside the age bound but outside the forecast.
        let fetched = local(2026, 8, 9, 20, 0);
        write(
            &path,
            &encode(
                &frederick(),
                &forecast(fetched),
                fetched.with_timezone(&Utc),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(
            load(&path, &frederick(), local(2026, 8, 10, 1, 0), MAX_AGE)
                .unwrap()
                .is_none()
        );
    }
}
