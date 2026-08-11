# virga

A terminal weather app. Current conditions, an eight-day forecast, three weeks
of daily highs you can browse a day at a time, and an hourly precipitation
chart — no account, no API key.

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

Press `p` for the hourly precipitation screen. Chance rises from a central
rule, forecast amount hangs below it:

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

A tall spike with nothing beneath it means "might drizzle". Tall above *and*
below means take the umbrella.

## Features

- **Current conditions** — temperature, feels-like, wind with gusts and
  direction, precipitation, daylight length, and US air quality index.
- **Eight-day forecast** with rain chance, max wind, UV index, sunrise and
  sunset.
- **Three weeks of daily highs** as a bar chart — fourteen days of history plus
  the forecast, so today has context rather than sitting alone.
- **Hourly precipitation** on `p`: chance and forecast amount mirrored around a
  central rule, with "next rain in 3 h" on the border, a 24-hour running total,
  and snow reported separately from rain.
- **Browse any day** with the arrow keys. The top pane becomes an inspector for
  the selected day, including how it compares to the period average.
- **City search** against Open-Meteo's geocoder.
- **Metric or imperial**, toggled live.
- **Responsive** — panes drop columns, stack, or sit side by side depending on
  the space available, down to a 34×12 terminal.

## Requirements

- **Rust 1.88 or later.** Not the 1.85 that edition 2024 implies: `ratatui`
  declares 1.88, and a dependency graph's floor is what actually binds.
- A terminal with Unicode support. Weather icons are emoji; a font without them
  will show replacement boxes in the forecast column but nothing else breaks.
- An internet connection. Nothing is cached between runs.

Tested on macOS (Apple Silicon). Linux and Windows are unverified — see
[Limitations](#limitations).

## Installation

Straight from the repository, no clone needed:

```bash
cargo install --git https://github.com/t-shahan/virga
```

Or from a local checkout:

```bash
git clone https://github.com/t-shahan/virga
cd virga
cargo install --path .
```

Either way the binary lands at `~/.cargo/bin/virga`. If that directory is on
your `PATH` — it is by default with a rustup install — you can then run:

```bash
virga
```

To update, re-run the install; it is a copy, not a link. To remove it,
`cargo uninstall virga-tui` — note that takes the *package* name, not the
binary name.

If you would rather not install it, `cargo run --release` works from the
project directory. Plain `cargo run` builds unoptimised and is noticeably
slower to render.

## Usage

| Key | Action |
|---|---|
| `q` / `Esc` | Quit |
| `Ctrl-C` | Quit, from any screen |
| `←` `→` | Move through the 22-day window; wraps at both ends |
| `n` / `Home` | Jump back to today |
| `p` | Hourly precipitation |
| `r` | Refetch the current location |
| `u` | Toggle metric / imperial |
| `l` | Search for a city |

On the precipitation screen:

| Key | Action |
|---|---|
| `←` `→` | Move an hour through the forecast; wraps at both ends |
| `↑` / `↓` | Forward / back a whole day, keeping the same time of day |
| `n` / `Home` | Jump back to the current hour |
| `b` / `p` / `Enter` / `Esc` | Back to the weather |
| `r` | Refetch the current location |
| `u` | Toggle metric / imperial |
| `l` | Search for a city |
| `q` | Quit |

The chart's centre rule marks the current hour (`┬`), the selected one (`═`)
and midnight (`┼`), so the three stay apart without relying on colour. The box
title carries the span on screen and the scale the lower half is drawn against
— the two halves are percentages against inches, and are not comparable by
height.

On the search screen:

| Key | Action |
|---|---|
| `Enter` | Search, then select the highlighted result |
| `↑` `↓` | Move through results |
| `Esc` | Back where you came from |

Choosing a city — or cancelling — returns to whichever screen the search was
opened from, so looking a place up from the precipitation screen leaves you on
the precipitation screen.

## Configuration

There is none yet. The startup location is a `Default` impl in `src/app.rs`:

```rust
impl Default for ActiveLocation {
    fn default() -> Self {
        Self {
            label: "Frederick, Maryland, United States".to_string(),
            lat: 39.414_27,
            lon: -77.410_54,
        }
    }
}
```

Change it and reinstall to start somewhere else. Locations chosen with `l`
apply for the session only — nothing is written to disk.

## Data

Weather, air quality and geocoding all come from
[Open-Meteo](https://open-meteo.com), which needs no API key.

### Attribution

Weather data by [Open-Meteo](https://open-meteo.com), licensed
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

Air-quality data is served by Open-Meteo from the Copernicus Atmosphere
Monitoring Service, and the
[Air Quality API](https://open-meteo.com/en/docs/air-quality-api) requires that
both be credited:

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

### Terms

Open-Meteo's free tier is **for non-commercial use only**, and is rate limited
to 10,000 calls per day, 5,000 per hour and 600 per minute. See
[Open-Meteo's terms](https://open-meteo.com/en/terms) and
[licence](https://open-meteo.com/en/license).

Each weather load makes **two** requests — forecast and air quality — and each
submitted city search makes a third, to the geocoding endpoint. Loads happen on
launch, on `r`, and on choosing a location. Held or repeated keys are not yet
deduplicated, so a leaned-on `r` can queue more requests than you intended.

### Privacy

The app stores nothing on disk: no account, no cache, no search history. It
does send the coordinates it is asked about, and any city name you search for,
to Open-Meteo. Open-Meteo states that its free-service logs may include IP
addresses and coordinates, retained for 90 days — see their
[terms and privacy](https://open-meteo.com/en/terms).

The committed default location is a public one in Frederick, Maryland. Change
it if you would rather not have a location compiled into your build, and
consider the same before publishing screenshots.

### What the numbers mean

- A missing or null reading degrades to a dash rather than failing the fetch.
- The AQI shown for **today** is the endpoint's current reading; for any other
  date it is the **maximum** of that day's hourly values. Both currently render
  under the same `AQI` label, which is a known ambiguity.
- Air-quality coverage runs out a couple of days short of the forecast horizon.
  Days beyond it show no AQI rather than a stale or zero one.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo +1.88.0 test --locked --all-targets   # the declared minimum
```

Two tests are `#[ignore]`d because they hit the live API; run them with
`cargo test -- --ignored`.

The tests concentrate on what breaks quietly: layout at awkward terminal sizes,
null and mismatched-length handling in API responses, unit conversion
boundaries, column alignment in both unit systems, day and hour navigation at
the wrap points, and HTTP timeouts against a loopback server that never
answers. Many render to a `TestBackend` and assert against the resulting buffer
rather than against the arithmetic that produced it.

Code is organised by responsibility:

```
src/
├── main.rs        startup and the event loop
├── app.rs         application state
├── events.rs      background worker and its messages
├── units.rs       metric/imperial conversion
├── weather/       API client, wire types, domain model
└── ui/            one module per pane, plus the block-digit font
```

`src/weather/` keeps the wire format (`dto.rs`) separate from the domain model
(`model.rs`), so a change to Open-Meteo's JSON touches one conversion rather
than the whole app. `src/ui/` contains no networking and `src/weather/`
contains no rendering.

## Limitations

- No configuration file; the default location requires a rebuild to change.
- Nothing is cached, so every launch fetches.
- Forecast text is English only.
- Terminals below 34×12 show a size warning instead of the interface. That
  minimum is a reduced current-conditions view — it does not have room for the
  forecast table and chart as well.
- Key release and repeat events are not filtered, which can duplicate input on
  Windows and lets a held key queue requests. Windows is not a supported
  platform yet for that reason.
- An in-flight search response is still accepted after the query has changed.
- Selection and "today" are distinguished by colour in the forecast table and
  the daily chart. The precipitation chart also marks them by shape; the others
  do not yet.
- There is no CI, so the only verified platform is the maintainer's.

## License

Copyright (C) 2026 Taylor Shahan.

Licensed under the **GNU General Public License v3.0 or later**. See
[LICENSE](LICENSE) for the full text.

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It is distributed in the hope that it will be useful, but WITHOUT ANY
WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A
PARTICULAR PURPOSE.

The licence above covers this program's own source. It is separate from the
licensing of the *data* it fetches, covered under [Data](#data).
