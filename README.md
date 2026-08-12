# virga

[![CI](https://github.com/t-shahan/virga/actions/workflows/ci.yml/badge.svg)](https://github.com/t-shahan/virga/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

A terminal weather app: current conditions, an eight-day forecast, three weeks
of daily highs, and an hourly precipitation chart — no account, no API key.

*Virga* is precipitation that evaporates before it reaches the ground. It is
also, most weeks, what the precipitation chart draws.

<img width="800" height="534" alt="CleanShot 2026-08-11 at 19 15 09" src="https://github.com/user-attachments/assets/413771fb-d6d8-4438-842f-14625039f806" />

Press `p` for the hourly precipitation screen. Chance rises from the centre
rule, forecast amount hangs below it — a tall spike with nothing beneath it
means "might drizzle"; tall above *and* below means take the umbrella.

## Features

- **Current conditions** — temperature, feels-like, wind with gusts and
  direction, precipitation, daylight length, and US air quality index.
- **Eight-day forecast** with rain chance, max wind, UV index, sunrise and sunset.
- **Three weeks of daily highs** — fourteen days of history plus the forecast,
  so today has context rather than sitting alone.
- **Hourly precipitation** — chance and amount mirrored around a centre rule,
  a next-rain countdown, a 24-hour running total, snow reported separately.
- **Browse any day** with the arrow keys; the top pane becomes an inspector for
  the selected day.
- **City search** against Open-Meteo's geocoder, **metric or imperial** toggled
  live, and a **responsive** layout down to a 34×12 terminal.

## Install

Requires **Rust 1.88 or later** — not the 1.85 edition 2024 implies, since
`ratatui` declares 1.88 — a terminal with Unicode support, and an internet
connection.

```bash
cargo install --git https://github.com/t-shahan/virga
virga
```

The binary lands at `~/.cargo/bin/virga`. Re-run the install to update, and
`cargo uninstall virga-tui` to remove it — that takes the *package* name, not
the binary name.

From a local checkout, `cargo run --release` works too; plain `cargo run`
builds unoptimised and is noticeably slower to render.

## Keys

| Key | Action |
|---|---|
| `←` `→` | Previous / next day — or hour, on the precipitation screen; wraps |
| `↑` `↓` | Precipitation screen: back / forward a day, keeping the time of day |
| `n` / `Home` | Jump back to now |
| `p` | Hourly precipitation — `b`, `Enter` or `Esc` to go back |
| `l` | Search for a city (`Enter` selects, `↑` `↓` move, `Esc` cancels) |
| `r` | Refetch the current location |
| `u` | Toggle metric / imperial |
| `q` / `Esc` / `Ctrl-C` | Quit |

The precipitation chart's centre rule marks the current hour (`┬`), the
selected one (`═`) and midnight (`┼`), so the three stay apart without relying
on colour. Its two halves are percentages against inches and are not comparable
by height; the box title carries the scale.

Choosing a city — or cancelling — returns to whichever screen the search was
opened from.

## Configuration

On its first run, Virga starts in New York City. Thereafter, it starts at the
last location whose weather loaded successfully. That location is kept in the
platform's per-user state/data directory.

## Development

```bash
cargo test                                  # 196 deterministic tests
cargo test -- --ignored                     # 2 live-API tests
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo +1.88.0 test --locked --all-targets   # the declared minimum
```

CI runs those on Linux, macOS and Windows, plus `cargo package` and a pinned
`cargo audit`; every job proves one thing, so a red build names the contract
that broke.

Tests concentrate on what breaks quietly: layout at awkward terminal sizes,
null and mismatched-length API responses, unit-conversion boundaries, day and
hour navigation at the wrap points, and HTTP timeouts against a loopback server
that never answers. Many render to a `TestBackend` and assert against the
resulting buffer rather than against the arithmetic that produced it.

Code is organised by responsibility: `src/ui/` does no networking and
`src/weather/` does no rendering, and the wire types in `weather/dto.rs` stay
separate from the domain model in `weather/model.rs`, so a change to
Open-Meteo's JSON touches one conversion rather than the whole app.

## Limitations

- There is no general configuration file, and weather is never cached. Every
  launch fetches fresh weather.
- Forecast text is English only.
- Terminals below 34×12 show a size warning instead of the interface.
- "Today" is distinguished by colour alone in the daily chart. The selection is
  not: every screen marks it by shape as well — a `>` in the forecast table's
  gutter, a `^` under the selected bar, and the precipitation chart's centre
  rule.
- Linux and Windows are covered by CI but have only been driven by hand on
  macOS. Passing unit tests does not validate console rendering, font fallback,
  or held-key behaviour on a real terminal.

## Data

Weather, air quality and geocoding all come from
[Open-Meteo](https://open-meteo.com), which needs no API key. Its free tier is
**for non-commercial use only** and is rate limited to 10,000 calls per day.
Each weather load makes two requests, and each submitted search a third.

Virga stores only the last successfully loaded location label and coordinates
locally, in its per-user state/data directory. It does not store weather
responses, searches, or history. Weather and air-quality requests send the
location coordinates to Open-Meteo; city searches submit their search text to
its geocoder. Open-Meteo's free-service logs may retain IP addresses and
coordinates for 90 days. See Open-Meteo's
[terms](https://open-meteo.com/en/terms) and
[licence](https://open-meteo.com/en/license).

A missing or null reading degrades to a dash rather than failing the fetch. The
AQI shown for today is the endpoint's current reading, labelled `AQI`; for any
other date it is that day's maximum, labelled `AQI max` — a day's worst hour is
not its prevailing air. Air-quality coverage runs out a couple of days short of
the forecast horizon.

### Attribution

Weather data by [Open-Meteo](https://open-meteo.com), licensed
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Air-quality data is
served by Open-Meteo from the Copernicus Atmosphere Monitoring Service (CAMS),
whose [Air Quality API](https://open-meteo.com/en/docs/air-quality-api)
requires that both be credited.

<details>
<summary>Full CAMS citation</summary>

> METEO FRANCE, Institut national de l'environnement industriel et des risques
> (Ineris), Aarhus University, Norwegian Meteorological Institute (MET Norway),
> Jülich Institut für Energie- und Klimaforschung (IEK), Institute of
> Environmental Protection – National Research Institute (IEP-NRI), Koninklijk
> Nederlands Meteorologisch Instituut (KNMI), Nederlandse Organisatie voor
> toegepast-natuurwetenschappelijk onderzoek (TNO), Swedish Meteorological and
> Hydrological Institute (SMHI), Finnish Meteorological Institute (FMI),
> Italian National Agency for New Technologies, Energy and Sustainable Economic
> Development (ENEA) and Barcelona Supercomputing Center (BSC) (2022): CAMS
> European air quality forecasts, ENSEMBLE data. Copernicus Atmosphere
> Monitoring Service (CAMS) Atmosphere Data Store (ADS). (Updated twice daily).
>
> All users of Open-Meteo data must provide a clear attribution to CAMS
> ENSEMBLE data provider as well as a reference to Open-Meteo.

</details>

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  <http://opensource.org/licenses/MIT>)

at your option. This covers the program's own source, which is separate from
the licensing of the data it fetches — see [Data](#data).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
