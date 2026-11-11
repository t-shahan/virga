//! A place's normals, kept on disk so the archive is asked once per place.
//!
//! Unlike the forecast cache this never goes stale: thirty years of history
//! do not change between launches. The archive request behind a table is
//! also the most expensive thing Virga asks of Open-Meteo, so the file is
//! what keeps the app a reasonable guest. Its own file and version for the
//! cache's reason: the remembered city must not depend on its shape.

use crate::app::ActiveLocation;
use crate::state;
use crate::weather::climate::Normals;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VERSION: u8 = 1;
const FILE: &str = "normals.json";
/// How many places are kept. A table is some twenty kilobytes, and someone
/// browsing cities should not grow the file without limit. The place read
/// or fetched longest ago goes first: `load` moves the place it returns to
/// the back, so a home city opened every day outlasts eight others looked
/// at once.
const MAX_PLACES: usize = 8;

#[derive(Serialize, Deserialize)]
struct Document {
    version: u8,
    /// Oldest first.
    places: Vec<Place>,
}

#[derive(Serialize, Deserialize)]
struct Place {
    lat: f64,
    lon: f64,
    normals: Normals,
}

impl Place {
    fn is(&self, location: &ActiveLocation) -> bool {
        self.lat == location.lat && self.lon == location.lon
    }
}

/// The one claim every version makes. See `state::surviving_document`.
#[derive(Deserialize)]
struct VersionEnvelope {
    version: u8,
}

pub fn path_beside(state: &Path) -> PathBuf {
    state.with_file_name(FILE)
}

/// The normals kept for `location`, if any, marking them the most recently
/// used.
///
/// `None`, silently, when there is no file, when a newer virga wrote it, and
/// when it holds nothing well-formed for this place. A file that is there
/// and cannot be read is an error for the caller to report. Failing to mark
/// the place is not: the table is in hand, and the cost is only its turn in
/// the eviction order.
pub fn load(path: &Path, location: &ActiveLocation) -> Result<Option<Normals>> {
    let Some(document) = read(path)? else {
        return Ok(None);
    };
    let found = document
        .places
        .iter()
        .rposition(|place| place.is(location) && place.normals.is_well_formed());
    let Some(index) = found else {
        return Ok(None);
    };
    let is_newest = index + 1 == document.places.len();
    let Some(place) = document.places.into_iter().nth(index) else {
        return Ok(None);
    };
    if !is_newest {
        let _ = save(path, location, &place.normals);
    }
    Ok(Some(place.normals))
}

fn read(path: &Path) -> Result<Option<Document>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let envelope: VersionEnvelope =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    if envelope.version > VERSION {
        return Ok(None);
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .with_context(|| format!("parse {}", path.display()))
}

/// Keep `normals` for `location`, beside whatever other places the file
/// already holds. Refuses to replace a file written by a newer virga, as the
/// cache does. A file that cannot be parsed is replaced: everything in it
/// can be fetched again, and leaving it would fail every save after.
pub fn save(path: &Path, location: &ActiveLocation, normals: &Normals) -> Result<()> {
    let _held = state::exclusive(path)?;
    state::ensure_not_newer(path, VERSION, "normals file")?;
    let mut places = read(path)
        .ok()
        .flatten()
        .map_or_else(Vec::new, |document| document.places);
    places.retain(|place| !place.is(location) && place.normals.is_well_formed());
    places.push(Place {
        lat: location.lat,
        lon: location.lon,
        normals: normals.clone(),
    });
    let surplus = places.len().saturating_sub(MAX_PLACES);
    places.drain(..surplus);

    let mut bytes = serde_json::to_vec(&Document {
        version: VERSION,
        places,
    })?;
    bytes.push(b'\n');
    state::replace_atomically(path, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(lat: f64) -> ActiveLocation {
        ActiveLocation {
            label: format!("Somewhere at {lat}"),
            lat,
            lon: -77.4,
        }
    }

    #[test]
    fn normals_come_back_for_the_place_they_were_saved_for() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        assert_eq!(load(&path, &place(39.0)).unwrap(), None, "no file yet");

        save(&path, &place(39.0), &Normals::flat(12.0, 5.0)).unwrap();
        save(&path, &place(52.0), &Normals::flat(8.0, 4.0)).unwrap();

        assert_eq!(
            load(&path, &place(39.0)).unwrap(),
            Some(Normals::flat(12.0, 5.0))
        );
        assert_eq!(
            load(&path, &place(52.0)).unwrap(),
            Some(Normals::flat(8.0, 4.0))
        );
        assert_eq!(load(&path, &place(10.0)).unwrap(), None);
    }

    /// The label is not the place: a city renamed by the geocoder is still
    /// the same coordinates, and its table is still its table.
    #[test]
    fn a_place_is_its_coordinates_whatever_it_is_called() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        save(&path, &place(39.0), &Normals::flat(12.0, 5.0)).unwrap();

        let renamed = ActiveLocation {
            label: "Elsewhere".to_string(),
            ..place(39.0)
        };
        assert!(load(&path, &renamed).unwrap().is_some());
    }

    #[test]
    fn saving_a_place_again_replaces_it_and_the_oldest_gives_way() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        for i in 0..MAX_PLACES {
            save(&path, &place(i as f64), &Normals::flat(1.0, 1.0)).unwrap();
        }

        // Saved again, the first place is the newest and no longer first out.
        save(&path, &place(0.0), &Normals::flat(2.0, 1.0)).unwrap();
        save(&path, &place(99.0), &Normals::flat(3.0, 1.0)).unwrap();

        assert_eq!(
            load(&path, &place(0.0)).unwrap(),
            Some(Normals::flat(2.0, 1.0))
        );
        assert_eq!(load(&path, &place(1.0)).unwrap(), None, "the oldest went");
        assert!(load(&path, &place(99.0)).unwrap().is_some());
        let document = read(&path).unwrap().unwrap();
        assert_eq!(document.places.len(), MAX_PLACES);
    }

    /// Eviction goes by use, not by first fetch: a place read back since
    /// is newer than one only ever saved.
    #[test]
    fn reading_a_place_keeps_it_from_being_the_next_to_go() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        for i in 0..MAX_PLACES {
            save(&path, &place(i as f64), &Normals::flat(1.0, 1.0)).unwrap();
        }

        assert!(load(&path, &place(0.0)).unwrap().is_some());
        save(&path, &place(99.0), &Normals::flat(3.0, 1.0)).unwrap();

        assert!(load(&path, &place(0.0)).unwrap().is_some(), "home stays");
        assert_eq!(
            load(&path, &place(1.0)).unwrap(),
            None,
            "the next oldest went"
        );
    }

    /// A row that parses and makes no sense is not shown: bounds out of
    /// order, or a number no thermometer has read.
    #[test]
    fn a_table_with_an_implausible_row_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        for row in [
            serde_json::json!({"mean_c": 10.0, "low_c": 15.0, "high_c": 5.0}),
            serde_json::json!({"mean_c": 1e300, "low_c": 0.0, "high_c": 1e300}),
        ] {
            save(&path, &place(39.0), &Normals::flat(12.0, 5.0)).unwrap();
            let mut document: serde_json::Value =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            document["places"][0]["normals"]["rows"][40] = row;
            std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();

            assert_eq!(load(&path, &place(39.0)).unwrap(), None);
        }
    }

    #[test]
    fn a_newer_file_is_neither_read_nor_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let newer = format!(r#"{{"version": {}, "shape": "unknown"}}"#, VERSION + 1);
        std::fs::write(&path, &newer).unwrap();

        assert_eq!(load(&path, &place(39.0)).unwrap(), None);
        let refused = save(&path, &place(39.0), &Normals::flat(12.0, 5.0)).unwrap_err();
        assert!(
            format!("{refused:#}").contains("newer virga"),
            "{refused:#}"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), newer);
    }

    /// A file that is not JSON is an error to report on the way in, and is
    /// replaced on the way out, since nothing in it cannot be fetched again.
    #[test]
    fn a_corrupt_file_is_reported_on_load_and_replaced_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        std::fs::write(&path, "not json").unwrap();

        assert!(load(&path, &place(39.0)).is_err());
        save(&path, &place(39.0), &Normals::flat(12.0, 5.0)).unwrap();
        assert!(load(&path, &place(39.0)).unwrap().is_some());
    }

    /// The file is the user's to edit and the disk's to damage. A table of
    /// the wrong length would index nowhere, so it is treated as absent.
    #[test]
    fn a_table_of_the_wrong_shape_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        save(&path, &place(39.0), &Normals::flat(12.0, 5.0)).unwrap();
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        document["places"][0]["normals"]["rows"] = serde_json::json!([null, null]);
        std::fs::write(&path, serde_json::to_vec(&document).unwrap()).unwrap();

        assert_eq!(load(&path, &place(39.0)).unwrap(), None);
    }
}
