# Remember Location Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Start Virga at the last location whose weather loaded successfully, with New York City as the first-run and recovery fallback.

**Architecture:** A focused `state` module owns the versioned JSON file, platform path selection, validation, and atomic replacement. `App` remains free of filesystem access: it accepts a startup location and reports when an accepted weather response supplies a location that the event loop should persist.

**Tech Stack:** Rust 2024, serde/serde_json, `directories` 6, `tempfile` 3, anyhow, the standard filesystem API, and the existing unit-test harness.

## Global Constraints

- Preserve the declared Rust 1.88 minimum and verify with the locked dependency graph.
- The fallback is exactly `New York City, New York, United States` at `40.7128, -74.0060`.
- Persist only the display label, latitude, and longitude; do not cache weather, queries, units, themes, or browsing history.
- Read and write failures must be non-fatal and must not corrupt the prior valid state.
- Accept only a non-empty trimmed label, finite coordinates, latitude in `-90..=90`, and longitude in `-180..=180`.
- Keep filesystem access out of `App` and networking out of `state`.
- Save only locations carried by accepted `Message::Loaded` responses; stale and failed responses must never save.

## File structure

- Create `src/state.rs`: versioned document, validation, platform path, load, and atomic save.
- Modify `src/app.rs`: New York default, startup-location constructor, and accepted-load outcome.
- Modify `src/main.rs`: load before terminal takeover, save after accepted loads, and defer save warnings until terminal restoration.
- Modify `Cargo.toml` and `Cargo.lock`: runtime JSON, directories, and atomic temporary-file support.
- Modify `README.md`: configuration, limitation, and privacy/storage claims.

---

### Task 1: New York fallback and explicit startup location

**Files:**
- Modify: `src/app.rs:28-114`

**Interfaces:**
- Produces: `ActiveLocation::default() -> ActiveLocation` for New York City.
- Produces: `App::with_location(location: ActiveLocation) -> App`.
- Preserves: `App::new() -> App` as the default-backed constructor used throughout existing tests.

- [ ] **Step 1: Write failing tests for the fallback and injected startup location**

Add to `src/app.rs` tests:

```rust
#[test]
fn the_builtin_location_is_new_york_city() {
    assert_eq!(
        ActiveLocation::default(),
        ActiveLocation {
            label: "New York City, New York, United States".to_string(),
            lat: 40.7128,
            lon: -74.0060,
        }
    );
}

#[test]
fn a_remembered_location_drives_the_initial_fetch() {
    let remembered = berlin();
    let mut app = App::with_location(remembered.clone());

    let Request::Fetch { location, .. } = app.initial_fetch() else {
        panic!("initial request was not a fetch")
    };

    assert_eq!(app.location, remembered);
    assert_eq!(location, remembered);
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run these separately:

```bash
cargo test the_builtin_location_is_new_york_city
cargo test a_remembered_location_drives_the_initial_fetch
```

Expected: the first test reports Frederick instead of New York, and the second fails because `App::with_location` does not exist.

- [ ] **Step 3: Implement the minimal constructors**

Change `ActiveLocation::default` to the exact New York values. Refactor construction so `new` delegates to `with_location`:

```rust
pub fn new() -> Self {
    Self::with_location(ActiveLocation::default())
}

pub fn with_location(location: ActiveLocation) -> Self {
    Self {
        screen: Screen::Weather,
        query: String::new(),
        results: Fetch::Idle,
        weather: Fetch::Loading,
        unit: Unit::Imperial,
        tick: 0,
        selected: 0,
        selected_day: 0,
        selected_hour: 0,
        location,
        pending: None,
        pending_search: None,
        next_request: 0,
        should_quit: false,
        search_return: Screen::Weather,
    }
}
```

- [ ] **Step 4: Run the app tests and verify GREEN**

Run: `cargo test app::tests`

Expected: all app tests pass with New York as the default.

- [ ] **Step 5: Commit the fallback behavior**

```bash
git add src/app.rs
git commit -m "feat: default startup weather to New York City"
```

---

### Task 2: Versioned state loading and validation

**Files:**
- Create: `src/state.rs`
- Modify: `src/main.rs:11-17`
- Modify: `Cargo.toml:14-28`
- Modify: `Cargo.lock`

**Interfaces:**
- Consumes: `crate::app::ActiveLocation`.
- Produces: `state::path() -> anyhow::Result<PathBuf>`.
- Produces: `state::load_from(path: &Path) -> anyhow::Result<Option<ActiveLocation>>`, used by `main` and colocated tests.

- [ ] **Step 1: Add state-file dependencies without upgrading unrelated crates**

Move `serde_json = "1.0.151"` into `[dependencies]`, then add `directories = "6.0.0"` and `tempfile = "3.27.0"` there. `tempfile` is used immediately by isolated state tests and by production atomic writes in Task 3. Run `cargo check --locked`; expect it to fail until `Cargo.lock` is updated. Run `cargo check` once to resolve the two new direct dependencies, inspect `git diff Cargo.lock` to ensure unrelated packages were not upgraded, then return to locked commands.

- [ ] **Step 2: Write failing state-loading tests**

Declare `mod state;` in `src/main.rs`. In the new `src/state.rs`, define only the test module first and use `tempfile::tempdir()` for isolation. Define the test-only helpers `location`, `write_raw`, and `write_document` directly in the module, then add tests:

```rust
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

    assert_eq!(load_from(&test.path().join("state.json")).unwrap(), Some(expected));
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
        ("future", r#"{"version":2,"location":{"label":"Berlin","lat":52.0,"lon":13.0}}"#),
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
```

Exercise non-finite coordinates by deserializing JSON with `null`, which serde must reject for `f64`, and also unit-test the validator directly with `f64::NAN` and both infinities.

```rust
#[test]
fn non_finite_coordinates_are_rejected() {
    for coordinate in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(validate(&location("Nowhere", coordinate, 0.0)).is_err());
        assert!(validate(&location("Nowhere", 0.0, coordinate)).is_err());
    }
}
```

- [ ] **Step 3: Run the state tests and verify RED**

Run: `cargo test state::tests`

Expected: compilation fails because `load_from`, the document types, and validation do not exist.

- [ ] **Step 4: Implement minimal loading and validation**

Use these concrete shapes:

```rust
const VERSION: u8 = 1;
const STATE_FILE: &str = "state.json";

#[derive(Deserialize, Serialize)]
struct StateDocument {
    version: u8,
    location: ActiveLocation,
}

fn validate(location: &ActiveLocation) -> anyhow::Result<()> {
    anyhow::ensure!(!location.label.trim().is_empty(), "location label is empty");
    anyhow::ensure!(location.lat.is_finite(), "latitude is not finite");
    anyhow::ensure!(location.lon.is_finite(), "longitude is not finite");
    anyhow::ensure!((-90.0..=90.0).contains(&location.lat), "latitude is out of range");
    anyhow::ensure!((-180.0..=180.0).contains(&location.lon), "longitude is out of range");
    Ok(())
}

fn load_from(path: &Path) -> anyhow::Result<Option<ActiveLocation>> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
    };
    let document: StateDocument = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse {}", path.display()))?;
    anyhow::ensure!(document.version == VERSION, "unsupported state version {}", document.version);
    validate(&document.location)?;
    Ok(Some(document.location))
}
```

Derive `Serialize` and `Deserialize` on `ActiveLocation`. Implement the platform path exactly as follows:

```rust
pub fn path() -> anyhow::Result<PathBuf> {
    let project = directories::ProjectDirs::from("com", "t-shahan", "virga")
        .context("the operating system has no user state directory")?;
    let directory = project
        .state_dir()
        .unwrap_or_else(|| project.data_local_dir());
    Ok(directory.join(STATE_FILE))
}
```

- [ ] **Step 5: Run state tests and verify GREEN**

Run: `cargo test state::tests`

Expected: all loading, version, and validation tests pass.

- [ ] **Step 6: Commit state loading**

```bash
git add Cargo.toml Cargo.lock src/app.rs src/main.rs src/state.rs
git commit -m "feat: load a remembered location from user state"
```

---

### Task 3: Atomic state replacement

**Files:**
- Modify: `src/state.rs`

**Interfaces:**
- Consumes: `StateDocument`, `validate`, and `ActiveLocation` from Task 2.
- Produces: `state::save_to(path: &Path, location: &ActiveLocation) -> anyhow::Result<()>`, used by `main` and colocated tests.

- [ ] **Step 1: Write failing save tests**

Add to `src/state.rs`:

```rust
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
```

- [ ] **Step 2: Run the save tests and verify RED**

Run these separately:

```bash
cargo test save_creates_a_document_that_load_can_read
cargo test a_failed_save_keeps_the_previous_valid_document
```

Expected: compilation fails because `save_to` does not exist.

- [ ] **Step 3: Implement atomic save**

Validate before touching the old file. Create the parent, serialize to a `tempfile::NamedTempFile` in that same parent, append a newline, flush and `sync_all`, then call `persist(path)`. Convert `PersistError` through its `error` field and attach the destination path as context:

```rust
fn save_to(path: &Path, location: &ActiveLocation) -> anyhow::Result<()> {
    validate(location)?;
    let parent = path.parent().context("state path has no parent")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create {}", parent.display()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("create temporary state in {}", parent.display()))?;
    serde_json::to_writer(
        temporary.as_file_mut(),
        &StateDocument { version: VERSION, location: location.clone() },
    )?;
    use std::io::Write as _;
    temporary.write_all(b"\n")?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

```

- [ ] **Step 4: Run all state tests and verify GREEN**

Run: `cargo test state::tests`

Expected: all state tests pass and no temporary files remain after successful replacement.

- [ ] **Step 5: Commit atomic persistence**

```bash
git add src/state.rs
git commit -m "feat: atomically save the active location"
```

---

### Task 4: Persist only accepted successful loads

**Files:**
- Modify: `src/app.rs:159-198`
- Modify: `src/main.rs:18-122`

**Interfaces:**
- Changes: `App::on_message(&mut self, message: Message) -> Option<&ActiveLocation>`.
- Consumes: `state::load_from`, `state::save_to`, `state::path`, and `App::with_location`.
- Produces: the existing `run` loop with an injected optional state path and `&mut Option<String>` that retains only the first save warning.

- [ ] **Step 1: Write failing app outcome tests**

Change the app test `deliver` helper to return whether a location was accepted, then add assertions to the existing stale/success cases:

```rust
fn deliver(app: &mut App, request: Request, weather: Weather) -> Option<ActiveLocation> {
    let Request::Fetch { id, location } = request else {
        panic!("not a fetch")
    };
    app.on_message(Message::Loaded { id, location, weather }).cloned()
}

#[test]
fn only_an_accepted_load_reports_a_location_to_persist() {
    let mut app = App::new();
    let stale = app.fetch(ActiveLocation::default());
    let current = app.fetch(berlin());

    assert_eq!(deliver(&mut app, stale, Weather::fixture(5, 2)), None);
    assert_eq!(
        deliver(&mut app, current, Weather::fixture(5, 2)),
        Some(berlin())
    );
}
```

Extend the existing failure test to assert `app.on_message(Message::LoadFailed { ... }).is_none()`.

- [ ] **Step 2: Run the focused outcome test and verify RED**

Run: `cargo test only_an_accepted_load_reports_a_location_to_persist`

Expected: compilation fails because `on_message` returns `()`.

- [ ] **Step 3: Return accepted locations from `on_message`**

Make every ignored, failed, and search branch return `None`. In the accepted `Message::Loaded` branch, update the existing fields and return `Some(&self.location)`. Update the few existing direct test calls to bind or assert the returned option so the `#[must_use]` value is not discarded.

- [ ] **Step 4: Run app tests and verify GREEN**

Run: `cargo test app::tests`

Expected: all app transition tests pass, including stale and failed response coverage.

- [ ] **Step 5: Write failing main-loop tests for startup and save warnings**

Extract pure orchestration helpers with injected paths:

```rust
fn startup_location(path: &Path) -> (ActiveLocation, Option<String>);
fn accept_message(app: &mut App, message: Message, path: &Path) -> Option<String>;
```

Add these tests, with a local `berlin()` constructor and `loaded(request)` helper that converts a `Request::Fetch` into `Message::Loaded` using `Weather::fixture(5, 2)`:

```rust
#[test]
fn remembered_location_wins_over_the_builtin_default() {
    let test = tempfile::tempdir().unwrap();
    let path = test.path().join("state.json");
    state::save_to(&path, &berlin()).unwrap();

    assert_eq!(startup_location(&path), (berlin(), None));
}

#[test]
fn broken_state_falls_back_with_a_warning() {
    let test = tempfile::tempdir().unwrap();
    let path = test.path().join("state.json");
    std::fs::write(&path, "{").unwrap();

    let (location, warning) = startup_location(&path);
    assert_eq!(location, ActiveLocation::default());
    assert!(warning.unwrap().contains("could not load remembered location"));
}

#[test]
fn an_accepted_load_is_persisted() {
    let test = tempfile::tempdir().unwrap();
    let path = test.path().join("state.json");
    let mut app = App::with_location(berlin());
    let message = loaded(app.initial_fetch());

    assert_eq!(accept_message(&mut app, message, &path), None);
    assert_eq!(state::load_from(&path).unwrap(), Some(berlin()));
}

#[test]
fn stale_and_failed_loads_do_not_replace_state() {
    let test = tempfile::tempdir().unwrap();
    let path = test.path().join("state.json");
    state::save_to(&path, &berlin()).unwrap();
    let mut app = App::new();
    let stale = app.initial_fetch();
    let current = app.initial_fetch();

    assert_eq!(accept_message(&mut app, loaded(stale), &path), None);
    let Request::Fetch { id, .. } = current else { panic!("not a fetch") };
    assert_eq!(
        accept_message(
            &mut app,
            Message::LoadFailed { id, error: "offline".to_string() },
            &path,
        ),
        None
    );
    assert_eq!(state::load_from(&path).unwrap(), Some(berlin()));
}
```

- [ ] **Step 6: Run the main tests and verify RED**

Run these separately:

```bash
cargo test remembered_location_wins_over_the_builtin_default
cargo test broken_state_falls_back_with_a_warning
cargo test an_accepted_load_is_persisted
cargo test stale_and_failed_loads_do_not_replace_state
```

Expected: compilation fails because the orchestration helpers do not exist.

- [ ] **Step 7: Implement startup load and accepted-load save**

Before `ratatui::init`, resolve `state::path` once. Make `state::load_from` and `state::save_to` `pub(crate)` so the injected-path orchestration helpers and the runtime share exactly one read path and one write path. Print any startup path/read warning immediately with a `virga:` prefix. Construct with `App::with_location(startup)`.

Implement the helpers as:

```rust
fn startup_location(path: &Path) -> (ActiveLocation, Option<String>) {
    match state::load_from(path) {
        Ok(Some(location)) => (location, None),
        Ok(None) => (ActiveLocation::default(), None),
        Err(error) => (
            ActiveLocation::default(),
            Some(format!("virga: could not load remembered location: {error:#}")),
        ),
    }
}

fn accept_message(app: &mut App, message: Message, path: &Path) -> Option<String> {
    let location = app.on_message(message)?;
    state::save_to(path, location)
        .err()
        .map(|error| format!("virga: could not remember location: {error:#}"))
}
```

In the message drain, use:

```rust
let save_warning = if let Some(path) = state_path {
    accept_message(&mut app, message, path)
} else {
    let _ = app.on_message(message);
    None
};
if warning.is_none() {
    *warning = save_warning;
}
```

Restore the terminal before printing the retained save warning. If platform path resolution failed, start in New York, print the warning before terminal initialization, and run without persistence.

- [ ] **Step 8: Run app and main tests and verify GREEN**

Run these suites separately:

```bash
cargo test app::tests
cargo test state::tests
cargo test remembered_location
cargo test accepted_load
cargo test stale_and_failed_loads
```

Expected: all persistence-boundary, startup fallback, and state tests pass.

- [ ] **Step 9: Commit lifecycle integration**

```bash
git add src/app.rs src/main.rs src/state.rs
git commit -m "feat: remember locations after successful weather loads"
```

---

### Task 5: Documentation and complete verification

**Files:**
- Modify: `README.md:71-75`
- Modify: `README.md:102-105`
- Modify: `README.md:116-127`

**Interfaces:**
- Documents the completed behavior; introduces no code interface.

- [ ] **Step 1: Update the user-facing contract**

Replace the configuration claim with text stating that Virga starts in New York City on first run and thereafter starts at the last location whose weather loaded successfully. State that the file is kept in the platform's per-user state/data directory.

Change the limitation from “No configuration file and no cache” to the narrower truth: there is no general configuration file and weather is never cached.

Replace “Nothing is stored on disk” with an explicit privacy statement: only the last successful location label and coordinates are stored locally; weather responses, searches, and history are not.

- [ ] **Step 2: Check formatting and the dependency floor**

Run:

```bash
cargo fmt --check
cargo check --locked
```

Expected: both commands exit successfully without changing files or the lockfile.

- [ ] **Step 3: Run Clippy with warnings denied**

Run: `cargo clippy --all-targets --locked -- -D warnings`

Expected: exit 0 with no warnings.

- [ ] **Step 4: Run the complete deterministic suite**

Run: `cargo test --locked --all-targets`

Expected: every deterministic test passes; the two live API tests remain ignored.

- [ ] **Step 5: Verify the declared MSRV when toolchain 1.88 is installed**

Run: `cargo +1.88.0 test --locked --all-targets`

Expected: exit 0. If toolchain 1.88 is unavailable locally, report that precise limitation and rely on the existing CI MSRV job rather than installing a toolchain without approval.

- [ ] **Step 6: Review the final diff for scope and privacy claims**

Run:

```bash
git diff --check
git diff --stat HEAD~4
git status --short
```

Confirm no weather payload, query, unit, theme, or browsing state is serialized, and no unrelated user changes are included.

- [ ] **Step 7: Commit documentation**

```bash
git add README.md
git commit -m "docs: explain remembered location state"
```
