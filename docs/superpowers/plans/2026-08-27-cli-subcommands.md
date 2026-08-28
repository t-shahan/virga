# CLI Subcommands Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `virga theme [name]` (list themes / persist a startup default), `virga update` (check the latest release, answer with an install-method-matched instruction), and a startup notice — the TUI probes for a newer release in the background and shows one dismissible muted line when it finds one — per `docs/superpowers/specs/2026-08-27-cli-subcommands-design.md`.

**Architecture:** A new `cli` module owns the grammar, moved out of `main.rs`. A new `update` module owns the release probe, with everything but one HTTP call a pure function; the subcommand and the startup notice are the same probe called from two places. `state` grows an optional `theme` field with merge-preserving saves. `theme.rs` is reused, not changed. `main` dispatches subcommands before the terminal, the network, or the state file are touched. The startup probe runs on a one-shot thread feeding the existing message channel — never the serial request queue.

**Tech Stack:** Rust 2024, the existing dependency set (ureq, serde/serde_json, directories, tempfile, anyhow). No new dependencies.

**Delivery:** Three pull requests. PR 1 is Tasks 1–5 (`virga theme`); PR 2 is Tasks 6–8 (`virga update`); PR 3 is Tasks 9–11 (the startup notice). Each stands alone and keeps CI green.

**Status:** PR 1 is implemented on this branch, with two deviations from Task 2 as written: the `update` grammar and the `Invocation::Usage` variant move to PR 2 with the subcommand they serve, so PR 1 ships no words it cannot answer; and `help`/`version` keep the flags' first-argument-wins leniency rather than erroring on trailing arguments — the strictness is only worth its surprise on `update`, where extra arguments could mean an intention the command will not carry out.

## Global Constraints

- Preserve the Rust 1.88 minimum and the locked dependency graph; add no crates.
- Subcommands answer and exit before `ratatui::init`, before any network lookup other than their own, and before any state write other than their own.
- Startup theme precedence, weakest to strongest: built-in default, persisted theme, `VIRGA_THEME`, the `t` key. An unusable `VIRGA_THEME` warns and falls back to the persisted theme.
- `t` remains session-only. Nothing the TUI does writes a theme.
- State documents are written at the lowest version that can carry them: version 2 whenever a location is present (with `theme` as an optional field old binaries ignore), version 3 only for a theme with no location. Reads accept versions 1–3; saves are read-merge-write and atomic; a failed write leaves the previous document intact.
- An unknown persisted theme name warns and is ignored; it must not take the remembered location down with it.
- Exit codes: 0 answered, 1 operational failure (network, unwritable state), 2 usage error. A typo never falls through into the full-screen application.
- `virga update` sends nothing but the request itself, and bounds it with the same timeout the weather client uses. The startup probe is the same request under the same bound.
- The startup probe never delays first paint, never rides the serial request queue, and fails silently — no network is the notice not appearing, not a warning.
- The notice renders in the `muted` role, is cleared by the next action on a screen that renders it (search never does, so search keys leave it standing) without consuming the action, and is skipped below the minimum terminal size. `VIRGA_UPDATE=off` skips the probe, with `VIRGA_GEOIP`'s grammar and forgiveness.
- Keep filesystem access out of `App`, networking out of `state`, and both out of `cli`.

## File structure

- Create `src/cli.rs`: `Invocation`, `parse_args`, `usage`, and their tests, moved from `main.rs` and extended.
- Create `src/update.rs`: tag-from-redirect resolution, version comparison, install-method classification, instruction text, notice composition.
- Modify `src/state.rs`: optional-field document, `Persisted`, `save_location`, `save_theme`, version-3 read/write.
- Modify `src/events.rs`: `Message::UpdateAvailable`, `spawn_update_check`.
- Modify `src/app.rs`: hold and clear the notice.
- Modify `src/ui/`: render the notice above the key bar.
- Modify `src/main.rs`: module wiring, subcommand dispatch, persisted-theme precedence in `startup_theme`, `VIRGA_UPDATE`, probe spawn, exit re-print.
- Modify `README.md` and `CHANGELOG.md` per task.

---

### Task 1: Extract the CLI into its own module

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `cli::Invocation`, `cli::parse_args`, `cli::usage` — the same items `main.rs` holds today, `pub(crate)`, behavior unchanged.

- [ ] **Step 1: Move, don't rewrite**

Move `Invocation`, `parse_args`, `usage`, and every test that exercises them (`no_arguments_runs_the_application` through `usage_names_the_binary_the_version_and_both_environment_variables`) from `src/main.rs` into a new `src/cli.rs`. Add `mod cli;` and update the `match` in `main` to `cli::parse_args` / `cli::usage`. The doc comment on `Invocation` about "no options that change how it runs" moves with it.

- [ ] **Step 2: Verify nothing changed**

Run: `cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked --all-targets`

Expected: everything green; `main.rs` shrinks by the moved block and its tests.

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "refactor: move argument parsing into a cli module"
```

---

### Task 2: The subcommand grammar

**Files:**
- Modify: `src/cli.rs`

**Interfaces:**
- Produces: `Invocation::Theme(Option<String>)` — `None` lists, `Some(name)` sets; the name is the post-`theme` arguments joined with single spaces.
- Produces: `Invocation::Update`.
- Produces: `Invocation::Usage(String)` for a recognized subcommand given arguments it does not take.
- Preserves: `Run`, `Version`, `Help`, `Unknown(String)`; `-h/--help/-V/--version` unchanged; `help` and `version` as subcommand spellings.

- [ ] **Step 1: Write failing grammar tests**

Add to `src/cli.rs` tests, in the existing table style:

```rust
#[test]
fn theme_alone_asks_for_the_list() {
    assert_eq!(parse_args(["theme"]), Invocation::Theme(None));
}

#[test]
fn theme_joins_its_arguments_so_multiword_names_need_no_quotes() {
    assert_eq!(
        parse_args(["theme", "tokyo", "night"]),
        Invocation::Theme(Some("tokyo night".to_string()))
    );
    assert_eq!(
        parse_args(["theme", "tokyo-night"]),
        Invocation::Theme(Some("tokyo-night".to_string()))
    );
}

#[test]
fn update_takes_no_arguments() {
    assert_eq!(parse_args(["update"]), Invocation::Update);
    assert!(matches!(parse_args(["update", "--install"]), Invocation::Usage(_)));
}

#[test]
fn help_and_version_work_as_words_too() {
    assert_eq!(parse_args(["help"]), Invocation::Help);
    assert_eq!(parse_args(["version"]), Invocation::Version);
}

#[test]
fn a_subcommand_typo_is_still_unknown() {
    assert_eq!(parse_args(["them"]), Invocation::Unknown("them".to_string()));
}
```

Also extend `usage_names_the_binary_the_version_and_both_environment_variables` to assert the text names `theme` and `update`.

- [ ] **Step 2: Run and verify RED**, e.g. `cargo test cli::`

- [ ] **Step 3: Implement**

Extend the `match` in `parse_args` (which now consumes the full iterator, not just the first argument) and rewrite `usage()` with the `Commands:` section from the design spec. Trailing arguments after `update`, `help`, `version`, or the flags become `Invocation::Usage` carrying a one-line complaint (e.g. `update takes no arguments`); `main` will print it, then the usage text, then exit 2 — the same path `Unknown` takes.

- [ ] **Step 4: Verify GREEN**, then the full gate: `cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked --all-targets`

- [ ] **Step 5: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: recognize theme and update subcommands"
```

---

### Task 3: A theme in the state document

**Files:**
- Modify: `src/state.rs`
- Modify: `src/main.rs` (call sites: `save_to` → `save_location`, `load_from` destructuring)

**Interfaces:**
- Produces: `state::Persisted { remembered: Option<Remembered>, theme: Option<Theme> }`.
- Produces: `state::load_from(path) -> Result<Persisted>` (replacing the `Option<Remembered>` return).
- Produces: `state::save_location(path, &Remembered) -> Result<()>` — preserves any theme on disk.
- Produces: `state::save_theme(path, Theme) -> Result<()>` — preserves any location on disk.
- Produces: a non-fatal warning channel for an unknown persisted theme name (a `warning: Option<String>` field on `Persisted`, matching the `(value, Option<String>)` idiom `main` already uses).

- [ ] **Step 1: Write failing tests**

Add to `src/state.rs` tests:

```rust
#[test]
fn a_theme_rides_in_a_version_2_document_beside_the_location() {
    // Written at version 2 because location is present: `theme` was optional
    // from the start, so this file remains readable by every binary.
}

#[test]
fn a_theme_with_no_location_is_a_version_3_document() {
    // save_theme on a fresh path writes version 3; load_from reads it back
    // with remembered = None.
}

#[test]
fn remembering_a_location_returns_the_document_to_version_2() {}

#[test]
fn saving_a_location_preserves_the_theme_and_vice_versa() {}

#[test]
fn version_1_and_2_documents_read_as_having_no_theme() {}

#[test]
fn an_unknown_theme_name_warns_but_keeps_the_location() {
    // {"version":2, location..., "theme":"solarized"} → remembered intact,
    // theme None, warning naming the value.
}

#[test]
fn a_version_3_document_with_neither_field_is_rejected() {}
```

Update `malformed_and_unsupported_documents_are_rejected`: the "future" case moves from version 3 to version 4.

- [ ] **Step 2: Run and verify RED**: `cargo test state::`

- [ ] **Step 3: Implement**

`StateDocument` gains `#[serde(default, skip_serializing_if = "Option::is_none")]` on `location`, `source`, and a new `theme: Option<String>`. `source_of` keeps its v1 migration; v2 requires a location; v3 requires at least one of location/theme. The theme is stored as `Theme::name()` and parsed with `Theme::from_name`. Both saves share one read-merge-write helper feeding the existing temp-file/persist tail; the version written is 2 when a location is present, else 3.

- [ ] **Step 4: Verify GREEN**, full gate.

- [ ] **Step 5: Commit**

```bash
git add src/state.rs src/main.rs
git commit -m "feat: persist a startup theme in the state document"
```

---

### Task 4: Wire `virga theme` and the precedence

**Files:**
- Modify: `src/main.rs`

**Interfaces:**
- Produces: `startup_theme(env: Option<&str>, persisted: Option<Theme>) -> Theme` — the precedence in one testable function.
- Produces: theme list/set handlers running before any terminal, network, or detection work.

- [ ] **Step 1: Write failing tests**

In `src/main.rs` tests:

```rust
#[test]
fn the_persisted_theme_outranks_the_default() {
    assert_eq!(startup_theme(None, Some(Theme::Nord)), Theme::Nord);
}

#[test]
fn the_environment_outranks_the_persisted_theme() {
    assert_eq!(startup_theme(Some("dracula"), Some(Theme::Nord)), Theme::Dracula);
}

#[test]
fn an_unusable_environment_value_falls_back_to_the_persisted_theme() {
    // The standing choice absorbs the typo.
    assert_eq!(startup_theme(Some("solarized"), Some(Theme::Nord)), Theme::Nord);
}
```

Plus pure-function tests for the list body (marker on the persisted theme, or on `default` with nothing persisted) and the set confirmation line — render both as `fn theme_listing(persisted: Option<Theme>) -> String` / `fn theme_set_message(theme: Theme) -> String` so they are testable without capturing stdout.

- [ ] **Step 2: RED**, then **Step 3: Implement**

In `main`, dispatch before the `VIRGA_THEME` read:

- `Theme(None)`: resolve the state path, load (a load failure warns and lists with no marker moved), print the listing, exit 0.
- `Theme(Some(name))`: `Theme::from_name` or exit 2 listing known themes (reuse the wording of the `VIRGA_THEME` warning); `state::save_theme` or exit 1 with the path in the error; print the confirmation.
- The run path loads state once and hands `persisted.theme` to `startup_theme` and `persisted.remembered` to `startup_location` — one read where today there is one read.

- [ ] **Step 4: GREEN**, full gate. Manually eyeball `cargo run -- theme`, `cargo run -- theme nord`, a second `cargo run -- theme`, and `cargo run -- theme solarized`.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: list and persist startup themes from the command line"
```

---

### Task 5: Documentation for the theme command (closes PR 1)

**Files:**
- Modify: `README.md`, `CHANGELOG.md`

- [ ] **Step 1: README**

- Themes section: replace "The theme is not written to disk; like the unit toggle, it lasts for the current session." with the `virga theme` story and the four-step precedence.
- Configuration section: the new second paragraph ("It answers two questions…" becomes the subcommand list).
- Limitations: shrink the no-configuration bullet — the startup theme is now persisted; units still last a session.
- Data and Privacy: the state file sentence gains "and, if you have set one, the name of your startup theme".

- [ ] **Step 2: CHANGELOG** under `Unreleased`: `virga theme` added, `VIRGA_THEME` fallback now lands on the persisted theme.

- [ ] **Step 3: Full gate** (`cargo package --locked` included — README ships in the crate), then commit:

```bash
git add README.md CHANGELOG.md
git commit -m "docs: explain the persisted startup theme"
```

---

### Task 6: The release probe

**Files:**
- Create: `src/update.rs`
- Modify: `src/main.rs` (add `mod update;`)

**Interfaces:**
- Produces: `update::latest_tag(base_url: &str) -> Result<String>` — one request, redirects disabled, tag parsed from the `Location` header. The base URL is injected so tests can point it at a loopback server, exactly as `weather::client` does.
- Produces: `update::Release` (a parsed `x.y.z` triple plus pre-release marker) with `Release::parse(tag: &str) -> Result<Release>` and `fn newer_than(&self, current: &Release) -> bool`.

- [ ] **Step 1: Write failing tests**

In `src/update.rs` tests:

```rust
#[test]
fn tags_parse_with_and_without_the_leading_v() {}

#[test]
fn an_rc_of_a_version_is_older_than_its_release() {
    // The repo has shipped v0.2.0-rc1; someone running it must be told 0.2.0
    // is an update.
}

#[test]
fn comparison_is_numeric_not_lexicographic() {
    // 0.10.0 is newer than 0.9.0.
}

#[test]
fn a_tag_that_is_not_a_version_is_an_error_not_a_guess() {}
```

Loopback tests following the `weather::client` pattern: a server answering 302 with a `Location` ending `/tag/v0.3.0` yields `v0.3.0`; a response with no `Location`, and a refused connection, are readable errors; a silent connection times out rather than hanging.

- [ ] **Step 2: RED**, then **Step 3: Implement**

`latest_tag` issues one GET to `{base}/releases/latest` via a ureq agent configured with redirects off and the weather client's timeout, and takes everything after `/tag/` in the `Location` header. `Release::parse` accepts `v?X.Y.Z(-suffix)?`; `newer_than` compares triples, then treats a pre-release as older than the bare triple.

- [ ] **Step 4: GREEN**, full gate.

- [ ] **Step 5: Commit**

```bash
git add src/update.rs src/main.rs
git commit -m "feat: resolve and compare the latest release tag"
```

---

### Task 7: Wire `virga update` and the install-method answer

**Files:**
- Modify: `src/update.rs`, `src/main.rs`

**Interfaces:**
- Produces: `update::InstallMethod` (`Homebrew`, `Cargo`, `Script`, `Download`) and `fn install_method(exe: &Path, home: Option<&Path>) -> InstallMethod` — pure, with the home directory injected for tests.
- Produces: `fn report(current: &Release, latest: &Release, method: InstallMethod, exe: &Path) -> String` — the full stdout body, testable without a network.

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn a_cellar_or_homebrew_path_means_brew() {
    // /opt/homebrew/Cellar/virga/0.2.0/bin/virga and
    // /home/linuxbrew/.linuxbrew/... both classify as Homebrew.
}

#[test]
fn the_cargo_bin_directory_means_cargo() {}

#[test]
fn anywhere_else_is_the_install_script() {
    // ~/.local/bin/virga → the plain one-liner; /usr/local/bin/virga → the
    // one-liner prefixed with VIRGA_INSTALL_DIR=/usr/local/bin.
}

#[test]
fn windows_is_pointed_at_the_releases_page() {}

#[test]
fn an_up_to_date_binary_is_told_so_in_one_line() {}

#[test]
fn an_available_update_names_both_versions_and_one_instruction() {}
```

- [ ] **Step 2: RED**, then **Step 3: Implement**

Classification: Windows first (`cfg(windows)` → `Download`); a path containing a `Cellar`, `homebrew`, or `.linuxbrew` component → `Homebrew`; a parent equal to `{home}/.cargo/bin` → `Cargo`; else `Script`. In `main`, `Invocation::Update` runs `latest_tag(GITHUB_URL)`, parses both versions (`CARGO_PKG_VERSION` as current), prints `report`, exits 0 — or exits 1 with the error, prefixed `virga:` like every other complaint. No state, no terminal.

- [ ] **Step 4: GREEN**, full gate. Manually run `cargo run -- update` once to see the live answer.

- [ ] **Step 5: Commit**

```bash
git add src/update.rs src/main.rs
git commit -m "feat: check for a newer release from the command line"
```

---

### Task 8: Documentation for the update command (closes PR 2)

**Files:**
- Modify: `README.md`, `CHANGELOG.md`

- [ ] **Step 1: README**

- "Updating and removing": add a line above the table — `virga update` tells you whether there is anything to update to, and which row of the table applies to you.
- Configuration: add `update` to the subcommand list.
- Data and Privacy: a paragraph for the check — one request to GitHub's release redirect, carrying nothing but the request itself, made only when you run `virga update`.

- [ ] **Step 2: CHANGELOG** under `Unreleased`.

- [ ] **Step 3: Full gate**, then commit:

```bash
git add README.md CHANGELOG.md
git commit -m "docs: explain the update check"
```

---

### Task 9: The background probe

**Files:**
- Modify: `src/update.rs`, `src/events.rs`, `src/main.rs`

**Interfaces:**
- Produces: `update::notice(current: &Release, latest: &Release, method: InstallMethod) -> Option<String>` — `None` when current is already newest, else the finished one-line notice text. Pure; shares its instruction wording with Task 7's `report`.
- Produces: `events::Message::UpdateAvailable { notice: String }`.
- Produces: `events::spawn_update_check(messages: Sender<Message>, probe: impl FnOnce() -> Option<String> + Send + 'static)` — a one-shot thread that sends at most one message and ends. The probe closure is injected so tests never open a socket.
- Produces: `checks_enabled(requested: Option<&str>) -> (bool, Option<String>)` in `main.rs` — `VIRGA_UPDATE`, sharing its parsing with `detection_enabled` (extract the common switch-parsing helper rather than copying it a third time).

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn no_newer_release_means_no_notice() {}

#[test]
fn the_notice_names_both_versions_and_points_at_the_subcommand() {
    // "update: virga 0.3.0 is available — run `virga update` for how"-shaped,
    // and short enough for a 34-column terminal to truncate gracefully.
}

#[test]
fn the_check_thread_sends_at_most_one_message_and_ends() {
    // A probe returning Some sends UpdateAvailable; one returning None sends
    // nothing; either way joining the thread terminates.
}

#[test]
fn virga_update_off_skips_the_probe() {
    // The same table detection_enabled's tests use, against VIRGA_UPDATE.
}
```

- [ ] **Step 2: RED**, then **Step 3: Implement**

`spawn_update_check` is a `thread::spawn` around `if let Some(notice) = probe() { let _ = messages.send(...); }` — a send after the app has quit is a dropped receiver, and ignoring that error is the whole shutdown story. In `main`, read `VIRGA_UPDATE` beside `VIRGA_GEOIP` (warning to the ordinary screen, before terminal takeover) and, inside `run`, spawn the check after the worker with a probe that calls `latest_tag` and `notice`. Failure inside the probe is `None`.

- [ ] **Step 4: GREEN**, full gate.

- [ ] **Step 5: Commit**

```bash
git add src/update.rs src/events.rs src/main.rs
git commit -m "feat: probe for a newer release in the background"
```

---

### Task 10: The notice on screen, and off it

**Files:**
- Modify: `src/app.rs`, `src/ui/` (the key-bar/legend site), `src/cli.rs` (usage text), `src/main.rs`

**Interfaces:**
- Produces: `App.update_notice: Option<String>`, set by `on_message(UpdateAvailable)`, cleared by the next `on_action` — which still performs the action.
- Produces: rendering of the notice in the `muted` role on the line above the key bar, only when the terminal is at or above the minimum size.
- Produces: exit re-print — a notice that was never cleared is printed after `ratatui::restore`, through the same channel warnings already use, so quitting immediately still delivers the news.

- [ ] **Step 1: Write failing tests**

App-level:

```rust
#[test]
fn an_update_message_raises_the_notice_and_marks_the_frame_dirty() {}

#[test]
fn the_next_action_clears_the_notice_and_still_acts() {
    // A right-arrow while the notice is up both advances the selection and
    // drops the notice: dismissal must never eat an input.
}

#[test]
fn quitting_with_the_notice_unseen_hands_it_back_for_the_ordinary_screen() {}
```

UI-level, with `TestBackend` like every other rendering test: the notice appears muted above the key bar at a comfortable size; a 34×12 terminal renders without panic and without the notice overwriting the forecast.

- [ ] **Step 2: RED**, then **Step 3: Implement**

One `Option<String>` on `App`, one clear at the top of `on_action`, one conditional line in the layout, one line in `usage()`'s Environment section for `VIRGA_UPDATE`, and the post-restore print in `main` beside the existing warning print.

- [ ] **Step 4: GREEN**, full gate. Manually: run against the real network on an older tag (`VIRGA_VERSION`-style trickery is not available here, so temporarily lower `version` in `Cargo.toml`, run, observe the notice, press a key, quit — then revert).

- [ ] **Step 5: Commit**

```bash
git add src/app.rs src/ui src/cli.rs src/main.rs
git commit -m "feat: show a dismissible notice when a newer release exists"
```

---

### Task 11: Documentation for the startup notice (closes PR 3)

**Files:**
- Modify: `README.md`, `CHANGELOG.md`

- [ ] **Step 1: README**

- "Updating and removing": the notice is how you find out; `virga update` is how you ask; the table is what you run.
- Configuration: `VIRGA_UPDATE` joins `VIRGA_THEME` and `VIRGA_GEOIP`.
- Data and Privacy: on each launch (unless `VIRGA_UPDATE=off`) Virga makes one request to GitHub's release-redirect endpoint, carrying nothing but the request itself; GitHub sees what any HTTPS request shows it. This joins ipapi.co in the list of non-Open-Meteo requests.
- Limitations: note that the notice is informational — Virga never updates itself.

- [ ] **Step 2: CHANGELOG** under `Unreleased`.

- [ ] **Step 3: Full gate**, then commit:

```bash
git add README.md CHANGELOG.md
git commit -m "docs: explain the startup update notice"
```

---

## Verification (each PR)

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

Plus, once per PR, the by-hand pass CI cannot do: run the new subcommands in a real terminal, then start the TUI and confirm the persisted theme appears and `t` still cycles from it without writing anything.

## Open questions (defaults chosen, cheap to reverse)

1. **Should `t` persist the theme it lands on?** Default: no — `t` is a preview, `virga theme` is the commitment. Reversing later is one `save_theme` call at the `on_action` boundary.
2. **Exit code for "update available"?** Default: 0 — the command answered. A distinct code (à la `brew outdated`) can be added without breaking exit-0 consumers.
3. **`virga update --install` for script installs?** Deferred; the design spec records the dependency and risk cost. Nothing in PR 2's shape blocks it.
4. **Notice as a banner rather than a pop-up box?** Default: banner — a modal would arrive asynchronously under the user's fingers and would rank news above the forecast, and its dismissal keystroke would have to be eaten. If it proves too quiet, a bordered overlay is a `ui` change only; the plumbing is identical.
5. **Remember dismissals across launches?** Default: no — the notice returns each launch until updated. A `dismissed: "0.3.0"` state field is the follow-up if it nags.
