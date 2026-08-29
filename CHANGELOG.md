# Changelog

All notable changes to Virga are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
Virga follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). While
the major version is 0, a minor bump may carry behaviour changes.

Every release must have a section here before it can be tagged: both
`scripts/release.sh` and the `verify` job in `.github/workflows/release.yml`
refuse to publish a version this file does not describe.

## [Unreleased]

### Added

- **An adaptive hourly weathergram.** Press `p` to inspect sky conditions,
  temperature, precipitation, and wind on one shared 12- to 48-hour timeline.
  Temperature draws as a filled silhouette above a rain band, the sky and wind
  rows annotate changes rather than every hour, and exact selected-hour
  details and the weekly precipitation strip remain when the terminal is tall
  enough.

### Changed

- The default terminal theme uses normal gray for muted labels so they remain
  legible in Terminal.app and Ghostty while staying quieter than readings.

## [0.4.0] - 2026-08-28

Two commands where Virga had none. It still takes no options that change how
it runs, but the command line now answers two questions without starting up:
which themes exist and which one the next launch will use, and whether this
copy is the newest one. Virga volunteers the second answer too, once per
launch, in a line above the key bar.

### Added

- **`virga theme`**: list the colour themes and mark the startup default, or
  persist one — `virga theme tokyo night` — so every later launch starts in
  it. The choice is stored in `state.json` beside the remembered location;
  `VIRGA_THEME` still overrides it for a single launch, and `t` still cycles
  themes for the session. `virga help` and `virga version` also now work as
  word spellings of `-h` and `-V`.
- **`virga update`**: check whether a newer release exists and print how to
  get it, matched to how this copy was installed — Homebrew, Cargo, the
  install script, or a download on Windows. One request to GitHub's release
  redirect answers it; the binary is never replaced in place.
- **A startup update notice.** Each launch makes the same release check in
  the background — on its own thread, never delaying the first frame — and
  shows one muted line above the key bar when a newer release exists. The
  next keypress on a screen that shows it clears it and still does its own
  job — keys typed into the city search leave it standing, since that screen
  never shows it; news that arrived before you quit is printed on the
  ordinary screen instead. `VIRGA_UPDATE=off` skips the check, and a launch
  that cannot reach GitHub — or that quits before GitHub answers — simply
  shows no notice.

### Changed

- The minimum supported Rust version rises from 1.88 to 1.89, for
  `File::lock`: state saves are now serialized across processes, so the app
  remembering a location while `virga theme` runs can no longer drop one
  side's change. A state file that cannot be read, or that was written by a
  newer Virga, now blocks saves instead of being replaced.
- An unusable `VIRGA_THEME` value now falls back to the persisted startup
  theme, when one is set, rather than to the built-in default.
- `README.md` documents the command line in a `Commands` table, beside the
  existing `Keys` table.

## [0.3.0] - 2026-08-27

Catppuccin comes back in two flavours, one of them the first palette Virga has
shipped for a light terminal, and the precipitation screen stops rebuilding its
week strip from scratch on every frame.

### Added

- **Catppuccin, in two flavours.** `catppuccin mocha` returns after being cut
  in 0.2.0, rebuilt around the mauve the scheme is actually known by — pastel
  purple bars, sky selection, yellow today — instead of the blue that vanished
  against the terminal default. `catppuccin latte` is the same scheme in dark
  ink and the first palette built for a light terminal background; every
  other theme assumes a dark one. Seven themes now.

### Changed

- Buffered input is drained before each redraw, so a held arrow key can no
  longer queue repeats faster than frames are drawn and keep scrolling after
  it is released.
- The precipitation screen's week strip is grouped once per frame and borrows
  its hours instead of cloning them, dropping several hundred allocations and
  timestamp parses from every redraw of the screen.

## [0.2.0] - 2026-08-27

The release that stops asking people to install Rust. Prebuilt binaries for five
platforms, a Homebrew tap, and an install script, alongside the colour themes,
remembered locations, and precipitation-screen work that had accumulated since
0.1.0.

### Added

- **Prebuilt binaries** for Linux (x86_64 and aarch64, statically linked
  against musl), macOS (Apple silicon and Intel), and Windows (x86_64). Every
  release carries a `SHA256SUMS` file and GitHub build provenance attestations.
- **Homebrew tap**: `brew install t-shahan/tap/virga`. The formula updates
  itself on every release, so `brew upgrade` is the whole update story.
- **Install script**: `curl -fsSL https://raw.githubusercontent.com/t-shahan/virga/main/install.sh | sh`.
  Verifies checksums before installing and re-running it upgrades in place.
- **Colour themes**, cycled with `t`: the terminal's own ANSI colours by
  default, plus Gruvbox Dark, Nord, Tokyo Night, and Dracula. All five set
  foregrounds only, so your terminal's background is left alone.
- **Remembered locations.** A city chosen with `l` is saved atomically after the
  weather load that confirms it, and Virga starts there next time.
- **IP-based startup location** when nothing is remembered yet, with
  `VIRGA_GEOIP=off` to skip the lookup. Falls back to New York City.
- **Axes and a week strip** on the precipitation screen, with `↑` and `↓` moving
  a day at a time while holding the time of day.

### Changed

- The default palette was renamed and the transient theme readout retired in
  favour of the key bar naming the theme you land on.
- Week-strip rows are counted by date rather than by elapsed hours, which fixes
  the row shown around daylight-saving transitions.
- `README.md` leads with the binary install paths. Building from source with
  `cargo install --git` is still supported and still documented, just no longer
  the only option.

### Removed

- The Catppuccin palette, in favour of themes that each define their own
  colours instead of sharing a base.

### Fixed

- Precipitation day arrows now point the way the week strip reads.
- The day arrows can reach today, and the selected hour is no longer dimmed.

## [0.1.0] - 2026-08-11

First release.

### Added

- Current conditions: temperature, feels-like, wind with gusts and direction,
  precipitation, daylight length, and US air quality index.
- Eight-day forecast with rain chance, max wind, UV index, sunrise, and sunset.
- Three weeks of daily highs, fourteen days of history plus the forecast.
- Hourly precipitation on `p`: chance and amount mirrored around a centre rule,
  a next-rain countdown, a 24-hour running total, and snow reported separately
  from rain.
- Day browsing with the arrow keys, turning the top pane into an inspector for
  the selected day.
- City search against Open-Meteo's geocoder, a live metric/imperial toggle, and
  a responsive layout down to a 34x12 terminal.
- Dual licensed MIT OR Apache-2.0. Weather data by Open-Meteo under CC BY 4.0.

[Unreleased]: https://github.com/t-shahan/virga/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/t-shahan/virga/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/t-shahan/virga/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/t-shahan/virga/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/t-shahan/virga/releases/tag/v0.1.0
