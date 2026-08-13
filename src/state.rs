use crate::app::{ActiveLocation, LocationSource, Remembered};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VERSION: u8 = 2;
const STATE_FILE: &str = "state.json";

#[derive(Deserialize, Serialize)]
struct StateDocument {
    version: u8,
    location: ActiveLocation,
    /// Absent in version 1, which had no concept of provenance. Its migration
    /// is `source_of`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<LocationSource>,
}

/// Which of the two things a saved location is: a city the user went looking
/// for, or a guess the app made on their behalf.
///
/// Version 1 recorded no such thing, and from the outside its two cases look
/// identical — except in one telling way. Version 1 wrote the compiled-in
/// fallback to disk on a first run, so a document holding *exactly* New York is
/// one nobody ever chose. Reading those as choices would make detection inert
/// for every existing user; reading every v1 document as a guess would throw
/// away the only choice the old format was capable of recording.
fn source_of(document: &StateDocument) -> Result<LocationSource> {
    match document.version {
        VERSION => document.source.context("no source"),
        1 if document.location == ActiveLocation::default() => Ok(LocationSource::Detected),
        1 => Ok(LocationSource::Chosen),
        other => anyhow::bail!("unsupported state version {other}"),
    }
}

fn validate(location: &ActiveLocation) -> Result<()> {
    anyhow::ensure!(!location.label.trim().is_empty(), "location label is empty");
    anyhow::ensure!(location.lat.is_finite(), "latitude is not finite");
    anyhow::ensure!(location.lon.is_finite(), "longitude is not finite");
    anyhow::ensure!(
        (-90.0..=90.0).contains(&location.lat),
        "latitude is out of range"
    );
    anyhow::ensure!(
        (-180.0..=180.0).contains(&location.lon),
        "longitude is out of range"
    );
    Ok(())
}

pub(crate) fn load_from(path: &Path) -> Result<Option<Remembered>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let document: StateDocument =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    let source = source_of(&document)?;
    validate(&document.location)?;
    Ok(Some(Remembered {
        location: document.location,
        source,
    }))
}

pub(crate) fn save_to(path: &Path, remembered: &Remembered) -> Result<()> {
    let Remembered { location, source } = remembered;
    // Unreachable while `App::remembered` holds — it never offers one — and
    // checked anyway, because writing the compiled-in default is precisely how
    // the old format let a first run masquerade as a choice.
    anyhow::ensure!(
        *source != LocationSource::Fallback,
        "the built-in fallback is not a remembered location"
    );
    validate(location)?;
    let parent = path.parent().context("state path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary state in {}", parent.display()))?;
    serde_json::to_writer(
        temporary.as_file_mut(),
        &StateDocument {
            version: VERSION,
            location: location.clone(),
            source: Some(*source),
        },
    )?;
    use std::io::Write as _;
    temporary.write_all(b"\n")?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

pub fn path() -> Result<PathBuf> {
    let project = directories::ProjectDirs::from("com", "t-shahan", "virga")
        .context("the operating system has no user state directory")?;
    let directory = project
        .state_dir()
        .unwrap_or_else(|| project.data_local_dir());
    Ok(directory.join(STATE_FILE))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ActiveLocation, LocationSource, Remembered};
    use std::path::Path;

    fn location(label: &str, lat: f64, lon: f64) -> ActiveLocation {
        ActiveLocation {
            label: label.to_string(),
            lat,
            lon,
        }
    }

    fn chosen(label: &str, lat: f64, lon: f64) -> Remembered {
        Remembered {
            location: location(label, lat, lon),
            source: LocationSource::Chosen,
        }
    }

    fn write_raw(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
    }

    /// A version 1 document — the format that had no source, and the one the
    /// migration has to keep reading.
    fn write_document(path: &Path, label: &str, lat: f64, lon: f64) {
        let body = serde_json::json!({
            "version": 1,
            "location": { "label": label, "lat": lat, "lon": lon },
        });
        std::fs::write(path, serde_json::to_vec(&body).unwrap()).unwrap();
    }

    #[test]
    fn a_saved_location_round_trips() {
        let test = tempfile::tempdir().unwrap();
        let expected = chosen("Berlin, Germany", 52.52437, 13.41053);
        write_raw(
            &test.path().join("state.json"),
            r#"{"version":2,"location":{"label":"Berlin, Germany","lat":52.52437,"lon":13.41053},"source":"chosen"}"#,
        );

        assert_eq!(
            load_from(&test.path().join("state.json")).unwrap(),
            Some(expected)
        );
    }

    #[test]
    fn both_sources_round_trip() {
        for source in [LocationSource::Chosen, LocationSource::Detected] {
            let test = tempfile::tempdir().unwrap();
            let path = test.path().join("state.json");
            let expected = Remembered {
                location: location("Berlin, Germany", 52.52437, 13.41053),
                source,
            };

            save_to(&path, &expected).unwrap();

            assert_eq!(load_from(&path).unwrap(), Some(expected));
        }
    }

    /// A version 1 file that is not the compiled-in default records a city the
    /// user went looking for. Detecting over it would throw away the only
    /// choice the old format was capable of recording.
    #[test]
    fn a_version_1_city_migrates_to_chosen() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        write_document(&path, "Berlin, Germany", 52.52437, 13.41053);

        assert_eq!(
            load_from(&path).unwrap().unwrap().source,
            LocationSource::Chosen
        );
    }

    /// Version 1 wrote the compiled-in fallback to disk on a first run, so a
    /// file holding exactly New York is one nobody ever chose. Reading it as a
    /// choice would make detection inert for every existing user.
    #[test]
    fn a_version_1_file_holding_only_the_default_migrates_to_detected() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let default = ActiveLocation::default();
        write_document(&path, &default.label, default.lat, default.lon);

        assert_eq!(
            load_from(&path).unwrap().unwrap().source,
            LocationSource::Detected
        );
    }

    /// Unreachable through `App`, and refused here anyway: writing the built-in
    /// fallback is exactly how the old format let a first run masquerade as a
    /// choice.
    #[test]
    fn the_builtin_fallback_cannot_be_saved() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");

        let result = save_to(
            &path,
            &Remembered {
                location: ActiveLocation::default(),
                source: LocationSource::Fallback,
            },
        );

        assert!(result.is_err());
        assert_eq!(load_from(&path).unwrap(), None, "and nothing was written");
    }

    #[test]
    fn a_missing_file_means_no_remembered_location() {
        let test = tempfile::tempdir().unwrap();
        assert_eq!(load_from(&test.path().join("state.json")).unwrap(), None);
    }

    #[test]
    fn malformed_and_unsupported_documents_are_rejected() {
        for (name, body) in [
            ("malformed", "{"),
            (
                "null-latitude",
                r#"{"version":1,"location":{"label":"Berlin","lat":null,"lon":13.0}}"#,
            ),
            (
                "future",
                r#"{"version":3,"location":{"label":"Berlin","lat":52.0,"lon":13.0}}"#,
            ),
            (
                "sourceless-current-version",
                r#"{"version":2,"location":{"label":"Berlin","lat":52.0,"lon":13.0}}"#,
            ),
        ] {
            let test = tempfile::tempdir().unwrap();
            let path = test.path().join("state.json");
            write_raw(&path, body);
            assert!(load_from(&path).is_err(), "{name} was accepted");
        }
    }

    #[test]
    fn invalid_locations_are_rejected() {
        for (name, label, lat, lon) in [
            ("empty-label", "   ", 40.0, -74.0),
            ("north", "North", 90.1, 0.0),
            ("south", "South", -90.1, 0.0),
            ("east", "East", 0.0, 180.1),
            ("west", "West", 0.0, -180.1),
        ] {
            let test = tempfile::tempdir().unwrap();
            let path = test.path().join("state.json");
            write_document(&path, label, lat, lon);
            assert!(load_from(&path).is_err(), "{name} was accepted");
        }
    }

    #[test]
    fn non_finite_coordinates_are_rejected() {
        for coordinate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(validate(&location("Nowhere", coordinate, 0.0)).is_err());
            assert!(validate(&location("Nowhere", 0.0, coordinate)).is_err());
        }
    }

    #[test]
    fn state_path_uses_the_state_file_name() {
        assert_eq!(path().unwrap().file_name().unwrap(), "state.json");
    }

    #[test]
    fn save_creates_a_document_that_load_can_read() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("nested").join("state.json");
        let expected = chosen("Berlin, Germany", 52.52437, 13.41053);

        save_to(&path, &expected).unwrap();

        assert_eq!(load_from(&path).unwrap(), Some(expected));
    }

    #[test]
    fn a_failed_save_keeps_the_previous_valid_document() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let previous = chosen("Berlin, Germany", 52.52437, 13.41053);
        save_to(&path, &previous).unwrap();

        let invalid = chosen("Nowhere", f64::NAN, 0.0);
        assert!(save_to(&path, &invalid).is_err());

        assert_eq!(load_from(&path).unwrap(), Some(previous));
    }
}
