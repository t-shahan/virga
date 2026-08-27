use crate::app::{ActiveLocation, LocationSource, Remembered};
use crate::theme::Theme;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Version 2 requires a location; the optional `theme` field rides along
/// unnoticed by binaries that predate it, which is what lets a themed
/// document stay readable everywhere. Version 3 exists for the one shape
/// version 2 cannot hold — a theme chosen before any weather has loaded, so
/// no location — and older binaries refuse it loudly rather than misread it.
/// A document is always written at the lowest version that can carry it.
const VERSION: u8 = 2;
const LOCATIONLESS_VERSION: u8 = 3;
const STATE_FILE: &str = "state.json";

#[derive(Deserialize, Serialize)]
struct StateDocument {
    version: u8,
    /// Absent only in version 3, which may carry a theme alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    location: Option<ActiveLocation>,
    /// Absent in version 1, which had no concept of provenance. Its migration
    /// lives in `remembered_of`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<LocationSource>,
    /// The startup theme `virga theme` persisted, by name. Optional in every
    /// version: most documents predate it or never set one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    theme: Option<String>,
}

/// Everything the state file had to say, with the parts that could not be
/// used already subtracted.
#[derive(Default)]
pub(crate) struct Persisted {
    pub remembered: Option<Remembered>,
    pub theme: Option<Theme>,
    /// A complaint about part of the document that did not stop the rest
    /// being used. Only an unknown theme name produces one today.
    pub warning: Option<String>,
}

/// The remembered location a document holds, if it holds a usable one.
///
/// Version 1 recorded no source, and from the outside its two cases look
/// identical — except in one telling way. Version 1 wrote the compiled-in
/// fallback to disk on a first run, so a document holding *exactly* New York
/// is one nobody ever chose. Reading those as choices would make detection
/// inert for every existing user; reading every v1 document as a guess would
/// throw away the only choice the old format was capable of recording.
fn remembered_of(document: &StateDocument) -> Result<Option<Remembered>> {
    anyhow::ensure!(
        (1..=LOCATIONLESS_VERSION).contains(&document.version),
        "unsupported state version {}",
        document.version
    );
    let Some(location) = &document.location else {
        // Only version 3 may omit the location, and only to carry a theme
        // instead; a document recording nothing at all records a bug.
        anyhow::ensure!(document.version == LOCATIONLESS_VERSION, "no location");
        anyhow::ensure!(document.theme.is_some(), "neither a location nor a theme");
        return Ok(None);
    };
    validate(location)?;
    let source = match document.version {
        1 if *location == ActiveLocation::default() => LocationSource::Detected,
        1 => LocationSource::Chosen,
        _ => document.source.context("no source")?,
    };
    Ok(Some(Remembered {
        location: location.clone(),
        source,
    }))
}

/// The theme, if the document names one this binary knows. An unknown name —
/// a theme dropped by some future version, or a hand-edit — is a warning and
/// nothing else: it must not take the remembered location down with it.
fn theme_of(document: &StateDocument) -> (Option<Theme>, Option<String>) {
    let Some(name) = &document.theme else {
        return (None, None);
    };
    match Theme::from_name(name) {
        Some(theme) => (Some(theme), None),
        None => (
            None,
            Some(format!(
                "virga: the state file names an unknown theme {name:?}; ignoring it."
            )),
        ),
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

pub(crate) fn load_from(path: &Path) -> Result<Persisted> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Persisted::default());
        }
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let document: StateDocument =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    let remembered = remembered_of(&document)?;
    let (theme, warning) = theme_of(&document);
    Ok(Persisted {
        remembered,
        theme,
        warning,
    })
}

/// What the document on disk still has to say, for a save that replaces only
/// half of it. Errors read as an empty document on purpose: an unreadable
/// file must not block saving over it — that is how corruption would become
/// permanent.
fn surviving(path: &Path) -> Persisted {
    load_from(path).unwrap_or_default()
}

pub(crate) fn save_location(path: &Path, remembered: &Remembered) -> Result<()> {
    let Remembered { location, source } = remembered;
    // Unreachable while `App::remembered` holds — it never offers one — and
    // checked anyway, because writing the compiled-in default is precisely how
    // the old format let a first run masquerade as a choice.
    anyhow::ensure!(
        *source != LocationSource::Fallback,
        "the built-in fallback is not a remembered location"
    );
    validate(location)?;
    let theme = surviving(path).theme;
    save_document(path, Some(remembered), theme)
}

pub(crate) fn save_theme(path: &Path, theme: Theme) -> Result<()> {
    let remembered = surviving(path).remembered;
    save_document(path, remembered.as_ref(), Some(theme))
}

fn save_document(path: &Path, remembered: Option<&Remembered>, theme: Option<Theme>) -> Result<()> {
    let version = match remembered {
        Some(_) => VERSION,
        None => LOCATIONLESS_VERSION,
    };
    let parent = path.parent().context("state path has no parent")?;
    std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary state in {}", parent.display()))?;
    serde_json::to_writer(
        temporary.as_file_mut(),
        &StateDocument {
            version,
            location: remembered.map(|remembered| remembered.location.clone()),
            source: remembered.map(|remembered| remembered.source),
            theme: theme.map(|theme| theme.name().to_string()),
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

    fn raw_version(path: &Path) -> u64 {
        let body: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        body["version"].as_u64().unwrap()
    }

    #[test]
    fn a_saved_location_round_trips() {
        let test = tempfile::tempdir().unwrap();
        let expected = chosen("Berlin, Germany", 52.52437, 13.41053);
        write_raw(
            &test.path().join("state.json"),
            r#"{"version":2,"location":{"label":"Berlin, Germany","lat":52.52437,"lon":13.41053},"source":"chosen"}"#,
        );

        let persisted = load_from(&test.path().join("state.json")).unwrap();

        assert_eq!(persisted.remembered, Some(expected));
        assert_eq!(persisted.theme, None);
        assert_eq!(persisted.warning, None);
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

            save_location(&path, &expected).unwrap();

            assert_eq!(load_from(&path).unwrap().remembered, Some(expected));
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
            load_from(&path).unwrap().remembered.unwrap().source,
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
            load_from(&path).unwrap().remembered.unwrap().source,
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

        let result = save_location(
            &path,
            &Remembered {
                location: ActiveLocation::default(),
                source: LocationSource::Fallback,
            },
        );

        assert!(result.is_err());
        assert_eq!(
            load_from(&path).unwrap().remembered,
            None,
            "and nothing was written"
        );
    }

    #[test]
    fn a_missing_file_means_nothing_remembered() {
        let test = tempfile::tempdir().unwrap();
        let persisted = load_from(&test.path().join("state.json")).unwrap();
        assert_eq!(persisted.remembered, None);
        assert_eq!(persisted.theme, None);
    }

    /// The document is written at the lowest version that can carry it: with
    /// a location present the theme is an optional field a version 2 reader
    /// simply ignores, so nobody's file becomes unreadable by installing a
    /// newer Virga and picking a theme.
    #[test]
    fn a_theme_rides_in_a_version_2_document_beside_the_location() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let berlin = chosen("Berlin, Germany", 52.52437, 13.41053);

        save_location(&path, &berlin).unwrap();
        save_theme(&path, Theme::Nord).unwrap();

        assert_eq!(raw_version(&path), 2);
        let persisted = load_from(&path).unwrap();
        assert_eq!(persisted.remembered, Some(berlin));
        assert_eq!(persisted.theme, Some(Theme::Nord));
    }

    /// A theme chosen before any weather has ever loaded has no city to sit
    /// beside — the one shape version 2 cannot hold.
    #[test]
    fn a_theme_with_no_location_is_a_version_3_document() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");

        save_theme(&path, Theme::Dracula).unwrap();

        assert_eq!(raw_version(&path), 3);
        let persisted = load_from(&path).unwrap();
        assert_eq!(persisted.remembered, None);
        assert_eq!(persisted.theme, Some(Theme::Dracula));
    }

    #[test]
    fn remembering_a_location_returns_the_document_to_version_2() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let berlin = chosen("Berlin, Germany", 52.52437, 13.41053);

        save_theme(&path, Theme::Dracula).unwrap();
        save_location(&path, &berlin).unwrap();

        assert_eq!(raw_version(&path), 2);
        let persisted = load_from(&path).unwrap();
        assert_eq!(persisted.remembered, Some(berlin));
        assert_eq!(persisted.theme, Some(Theme::Dracula));
    }

    /// The two saves replace only their own half of the document. Losing the
    /// theme because the weather loaded — or the city because a theme was
    /// picked — would make the two features fight.
    #[test]
    fn saving_a_location_preserves_the_theme_and_vice_versa() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let berlin = chosen("Berlin, Germany", 52.52437, 13.41053);
        let reykjavik = chosen("Reykjavík, Iceland", 64.14659, -21.94223);

        save_location(&path, &berlin).unwrap();
        save_theme(&path, Theme::Nord).unwrap();
        save_location(&path, &reykjavik).unwrap();

        let persisted = load_from(&path).unwrap();
        assert_eq!(persisted.remembered, Some(reykjavik));
        assert_eq!(persisted.theme, Some(Theme::Nord));
    }

    #[test]
    fn version_1_and_2_documents_read_as_having_no_theme() {
        let test = tempfile::tempdir().unwrap();
        let v1 = test.path().join("v1.json");
        write_document(&v1, "Berlin, Germany", 52.52437, 13.41053);
        let v2 = test.path().join("v2.json");
        write_raw(
            &v2,
            r#"{"version":2,"location":{"label":"Berlin","lat":52.0,"lon":13.0},"source":"chosen"}"#,
        );

        for path in [v1, v2] {
            let persisted = load_from(&path).unwrap();
            assert!(persisted.remembered.is_some());
            assert_eq!(persisted.theme, None);
            assert_eq!(persisted.warning, None);
        }
    }

    /// A theme this binary does not know — dropped by a future version, or a
    /// hand-edit — is a warning and nothing else. It must not take the
    /// remembered location down with it.
    #[test]
    fn an_unknown_theme_name_warns_but_keeps_the_location() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        write_raw(
            &path,
            r#"{"version":2,"location":{"label":"Berlin","lat":52.0,"lon":13.0},"source":"chosen","theme":"solarized"}"#,
        );

        let persisted = load_from(&path).unwrap();

        assert!(persisted.remembered.is_some());
        assert_eq!(persisted.theme, None);
        assert!(persisted.warning.unwrap().contains("solarized"));
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
                r#"{"version":4,"location":{"label":"Berlin","lat":52.0,"lon":13.0}}"#,
            ),
            (
                "sourceless-current-version",
                r#"{"version":2,"location":{"label":"Berlin","lat":52.0,"lon":13.0}}"#,
            ),
            (
                "locationless-current-version",
                r#"{"version":2,"theme":"nord"}"#,
            ),
            ("empty-version-3", r#"{"version":3}"#),
            (
                "sourceless-version-3",
                r#"{"version":3,"location":{"label":"Berlin","lat":52.0,"lon":13.0},"theme":"nord"}"#,
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

        save_location(&path, &expected).unwrap();

        assert_eq!(load_from(&path).unwrap().remembered, Some(expected));
    }

    #[test]
    fn a_failed_save_keeps_the_previous_valid_document() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let previous = chosen("Berlin, Germany", 52.52437, 13.41053);
        save_location(&path, &previous).unwrap();

        let invalid = chosen("Nowhere", f64::NAN, 0.0);
        assert!(save_location(&path, &invalid).is_err());

        assert_eq!(load_from(&path).unwrap().remembered, Some(previous));
    }

    /// A corrupt file must not block saving over it — that is how corruption
    /// would become permanent. What could not be read is gone, and that is
    /// the least bad option on the table.
    #[test]
    fn saving_over_a_corrupt_file_replaces_it() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        write_raw(&path, "{");

        save_theme(&path, Theme::Nord).unwrap();

        let persisted = load_from(&path).unwrap();
        assert_eq!(persisted.theme, Some(Theme::Nord));
        assert_eq!(persisted.remembered, None);
    }
}
