# weather

A terminal weather app. Current conditions, an eight-day forecast, and three
weeks of daily highs you can browse a day at a time — no account, no API key.

```
┌FREDERICK, MARYLAND────────────────────────────────────────────────Clear sky┐
│             ███████ ███████          feels like  79°F                      │
│                  ██      ██          high / low  93°F / 75°F               │
│                  ██      ██ °F       rain        0.10 in over 3 h          │
│                  ██      ██          wind        7, gusts 19 mph NW        │
│                  ██      ██          daylight    13h 42m                   │
│                                      air quality 54 · Moderate             │
└6°F above the 22-day average───────────────────────────────────────────Today┘
┌Forecast────────────────────────────────────────────────────────────────────┐
│      day     high     low   rain    wind     uv   sunrise   sunset         │
│      Today   93°F    75°F     2%   8 mph      7     06:00    20:02   ☀️     │
│      Sun     95°F    77°F    32%  12 mph      7     06:01    20:00   ⛅     │
│      Mon     97°F    79°F     5%   9 mph      7     06:02    19:59   ☀️     │
└────────────────────────────────────────────────────────────────────────────┘
┌Daily Highs · 68–106°F──────────────────────────────────────────────────────┐
│                                                      ▃▃ ▅▅ ██ ██ ██ ██     │
│               ▁▁ ▃▃ ▆▆ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██     │
│      ▁▁ ▄▄ ▆▆ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██ ██     │
└────────────────────────────────────────────────────────────────────────────┘
  [q] quit   [←→] day   [n] now   [r] refresh   [u] units   [l] location
```

## Features

- **Current conditions** — temperature, feels-like, wind with gusts and
  direction, rainfall, daylight length, and US air quality index.
- **Eight-day forecast** with rain chance, max wind, UV index, sunrise and
  sunset.
- **Three weeks of daily highs** as a bar chart — fourteen days of history plus
  the forecast, so today has context rather than sitting alone.
- **Browse any day** with the arrow keys. The top pane becomes an inspector for
  the selected day, including how it compares to the period average.
- **City search** against Open-Meteo's geocoder.
- **Metric or imperial**, toggled live.
- **Responsive** — panes drop columns, stack, or sit side by side depending on
  the space available, down to a 34×12 terminal.

## Requirements

- **Rust 1.85 or later** (the crate uses edition 2024)
- A terminal with Unicode support. Weather icons are emoji; a font without them
  will show replacement boxes in the forecast column but nothing else breaks.
- An internet connection. Nothing is cached between runs.

## Installation

From source, which is currently the only route:

```bash
git clone <repository-url>
cd weather_tui
cargo install --path .
```

That builds in release mode and copies the binary to `~/.cargo/bin/weather`.
If that directory is on your `PATH` — it is by default with a rustup install —
you can then run:

```bash
weather
```

To update after pulling changes, re-run `cargo install --path .`; the install is
a copy, not a link. To remove it, `cargo uninstall weather_tui` (note that takes
the *package* name, not the binary name).

If you would rather not install it, `cargo run --release` works from the project
directory. Plain `cargo run` builds unoptimised and is noticeably slower to
render.

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
| `↑` `↓` | Move a whole day |
| `n` / `Home` | Jump back to the current hour |
| `b` / `Enter` / `Esc` | Back to the weather |
| `r` | Refetch the current location |
| `u` | Toggle metric / imperial |
| `l` | Search for a city |
| `q` | Quit |

Chance rises from the centre rule, forecast amount hangs below it, and the
box title carries the span and the scale the lower half is drawn against.
The rule marks the current hour (`┬`), the selected one (`═`) and midnight
(`┼`), so the three stay apart without relying on colour.

On the search screen:

| Key | Action |
|---|---|
| `Enter` | Search, then select the highlighted result |
| `↑` `↓` | Move through results |
| `Esc` | Back to the weather |

## Configuration

There is none yet. The startup location is a constant in `src/main.rs`:

```rust
const DEFAULT_LOCATION: Place = Place {
    name: "Frederick, Maryland, United States",
    lat: 39.41427,
    lon: -77.41054,
};
```

Change it and reinstall to start somewhere else. Locations chosen with `l` apply
for the session only — nothing is written to disk.

## Data

Weather, air quality and geocoding all come from
[Open-Meteo](https://open-meteo.com), which needs no API key.

Open-Meteo's free tier is **for non-commercial use only** and its data is
licensed **[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/)**, so
attribution is required if you redistribute it. The free tier is also rate
limited — 10,000 calls per day, 5,000 per hour, 600 per minute — which this app
comes nowhere near, since it fetches only on launch, on `r`, and on changing
location.

Three endpoints are used: forecast, air quality, and geocoding. A missing or
null reading degrades to a dash rather than failing the fetch.

## Development

```bash
cargo test            # 55 tests
cargo clippy          # expected to be clean
cargo fmt
```

The tests are concentrated on the things that break quietly: layout at awkward
terminal sizes, null handling in API responses, unit conversion boundaries, and
column alignment. Several render to a `TestBackend` and assert against the
resulting buffer.

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
than the whole app. `src/ui/` contains no networking and `src/weather/` contains
no rendering.

## Limitations

- No configuration file; the default location requires a rebuild to change.
- Nothing is cached, so every launch fetches.
- Forecast text is English only.
- Terminals below 34×12 show a size warning instead of the interface.

## License

**TODO — choose before publishing.** MIT or Apache-2.0 are the usual choices for
Rust projects, and dual-licensing under both is the ecosystem convention. Add a
`LICENSE` file and replace this section.

Note this is separate from the licensing of the *data*, covered under
[Data](#data) above.
