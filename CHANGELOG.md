# Changelog

All notable changes to Virga are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
Virga follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). While
the major version is 0, a minor bump may carry behaviour changes.

Every release must have a section here before it can be tagged: both
`scripts/release.sh` and the `verify` job in `.github/workflows/release.yml`
refuse to publish a version this file does not describe.

## [Unreleased]

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

[Unreleased]: https://github.com/t-shahan/virga/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/t-shahan/virga/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/t-shahan/virga/releases/tag/v0.1.0
