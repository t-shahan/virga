use crate::app::ActiveLocation;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VERSION: u8 = 1;
const STATE_FILE: &str = "state.json";

#[derive(Deserialize, Serialize)]
struct StateDocument {
    version: u8,
    location: ActiveLocation,
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

pub(crate) fn load_from(path: &Path) -> Result<Option<ActiveLocation>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let document: StateDocument =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    anyhow::ensure!(
        document.version == VERSION,
        "unsupported state version {}",
        document.version
    );
    validate(&document.location)?;
    Ok(Some(document.location))
}

pub(crate) fn save_to(path: &Path, location: &ActiveLocation) -> Result<()> {
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
    use crate::app::ActiveLocation;
    use std::path::Path;

    fn location(label: &str, lat: f64, lon: f64) -> ActiveLocation {
        ActiveLocation {
            label: label.to_string(),
            lat,
            lon,
        }
    }

    fn write_raw(path: &Path, body: &str) {
        std::fs::write(path, body).unwrap();
    }

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
        let expected = location("Berlin, Germany", 52.52437, 13.41053);
        write_raw(
            &test.path().join("state.json"),
            r#"{"version":1,"location":{"label":"Berlin, Germany","lat":52.52437,"lon":13.41053}}"#,
        );

        assert_eq!(
            load_from(&test.path().join("state.json")).unwrap(),
            Some(expected)
        );
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
        let expected = location("Berlin, Germany", 52.52437, 13.41053);

        save_to(&path, &expected).unwrap();

        assert_eq!(load_from(&path).unwrap(), Some(expected));
    }

    #[test]
    fn a_failed_save_keeps_the_previous_valid_document() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let previous = location("Berlin, Germany", 52.52437, 13.41053);
        save_to(&path, &previous).unwrap();

        let invalid = location("Nowhere", f64::NAN, 0.0);
        assert!(save_to(&path, &invalid).is_err());

        assert_eq!(load_from(&path).unwrap(), Some(previous));
    }
}
