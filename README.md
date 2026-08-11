# virga

[![CI](https://github.com/t-shahan/virga/actions/workflows/ci.yml/badge.svg)](https://github.com/t-shahan/virga/actions/workflows/ci.yml)
[![License: GPL-3.0-or-later](https://img.shields.io/badge/license-GPL--3.0--or--later-blue.svg)](LICENSE)

A terminal weather app: current conditions, an eight-day forecast, three weeks
of daily highs, and an hourly precipitation chart — no account, no API key.

*Virga* is precipitation that evaporates before it reaches the ground. It is
also, most weeks, what the precipitation chart draws.

```
┌FREDERICK, MARYLAND, UNITED STATES────────────────Overcast───AQI 66 Moderate┐
│           ███████    ██            feels like  76°F                        │
│                ██    ██            high / low  90°F / 69°F                 │
│                ██    ██   °F       rain        65% · 0.03 in / 1 h         │
│                ██    ██            wind        16, gusts 19 mph SE         │
│                ██    ██            daylight    13h 54m                     │
└4°F above the 22-day average───────────────────────────────────────────Today┘
┌Forecast────────────────────────────────────────────────────────────────────┐
│      day     high     low   rain    wind     uv   sunrise   sunset         │
│      Today   90°F    69°F    65%  16 mph      8     06:17    20:11   🌧     │
│      Tue     89°F    70°F    18%  10 mph      7     06:18    20:10   ☁     │
│      Wed     86°F    69°F    23%  11 mph      8     06:19    20:09   🌧     │
│      Thu     87°F    67°F    18%  12 mph      7     06:20    20:08   🌧     │
│      Fri     87°F    65°F    35%  15 mph      7     06:21    20:06   ☁     │
│      Sat     85°F    57°F     6%   9 mph      7     06:22    20:05   ☁     │
│      Sun     94°F    63°F    39%  14 mph      7     06:23    20:04   🌧     │
│      Mon     90°F    64°F    39%  18 mph      7     06:24    20:02   🌧     │
└────────────────────────────────────────────────────────────────────────────┘
┌Daily Highs · 78–94°F───────────────────────────────────────────────────────┐
│                                       ▇▇                         ██        │
│                                    ██ ██    ██ ▇▇ ▃▃             ██ ▅▅     │
│      ██          ▅▅ ▅▅    ▇▇ ▄▄    ██ ██ ▄▄ ██ ██ ██ ▆▆ ██ ██ ▂▂ ██ ██     │
│      ██ ▇▇    ▂▂ ██ ██    ██ ██ ▆▆ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██     │
│      ██ ██ ██ ██ ██ ██ ▆▆ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██     │
└────────────────────────────────────────────────────────────────────────────┘
  [q] quit   [←→] day   [n] now   [p] precip   [r] refresh   [u] units
  [l] location
```

Press `p` for the hourly precipitation screen. Chance rises from the centre
rule, forecast amount hangs below it — a tall spike with nothing beneath it
means "might drizzle"; tall above *and* below means take the umbrella.

```
┌FREDERICK, MARYLAND, UNITED STATES──────────────────────────────────Overcast┐
│               ███████             amount      none expected                │
│               ██   ██             temperature 71°F                         │
│               ███████ %           24 h total  none expected                │
│               ██   ██             24 h peak   18% at 4 PM                  │
│               ███████             wet hours   0 of 24                      │
└next rain Wed 12 Aug, 11 PM─────────────────────────────Mon 10 Aug, 10:00 PM┘
┌Precipitation · next 25 h · chance ▲ · amount ▼ 0–0.28 in───────────────────┐
│                                                                            │
│                                                       ▁▁                   │
│ ▄▄ ▄▄    ▁▁ ▂▂ ▄▄ ▄▄ ▄▄ ▄▄ ▃▃ ▃▃ ▃▃ ▂▂ ▃▃ ▆▆ ▇▇ ▅▅ ▇▇ ██ ██ ▅▅ ▃▃ ▆▆ ▅▅ ▂▂ │
│ ══ ── ┼┼ ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── ── │
│                                                                            │
└no rain or snow in the next 25 h────────────────────────────────────────────┘
  [q] quit   [b] back   [←→] hour   [↑↓] day   [n] now   [r] refresh
  [u] units
```

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

There is none yet. The startup location is a `Default` impl in `src/app.rs`;
change it and reinstall to start somewhere else. Cities chosen with `l` apply
for the session only — nothing is written to disk.

## Development

```bash
cargo test                                  # 176 deterministic tests
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

- No configuration file and no cache: every launch fetches, and the default
  location needs a rebuild to change.
- Forecast text is English only.
- Terminals below 34×12 show a size warning instead of the interface.
- Selection and "today" are distinguished by colour alone in the forecast table
  and the daily chart; only the precipitation chart also marks them by shape.
- Linux and Windows are covered by CI but have only been driven by hand on
  macOS. Passing unit tests does not validate console rendering, font fallback,
  or held-key behaviour on a real terminal.

## Data

Weather, air quality and geocoding all come from
[Open-Meteo](https://open-meteo.com), which needs no API key. Its free tier is
**for non-commercial use only** and is rate limited to 10,000 calls per day.
Each weather load makes two requests, and each submitted search a third.

Nothing is stored on disk — no account, no cache, no history — but the
coordinates and city names you ask about are sent to Open-Meteo, whose
free-service logs may retain IP addresses and coordinates for 90 days. See
Open-Meteo's [terms](https://open-meteo.com/en/terms) and
[licence](https://open-meteo.com/en/license).

A missing or null reading degrades to a dash rather than failing the fetch. The
AQI shown for today is the endpoint's current reading; for any other date it is
that day's maximum, and both render under the same label. Air-quality coverage
runs out a couple of days short of the forecast horizon.

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

Copyright (C) 2026 Taylor Shahan. Licensed under the
**GNU General Public License v3.0 or later**; see [LICENSE](LICENSE) for the
full text. It covers this program's own source, which is separate from the
licensing of the data it fetches — see [Data](#data).
