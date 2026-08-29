# Virga

[![CI](https://github.com/t-shahan/virga/actions/workflows/ci.yml/badge.svg)](https://github.com/t-shahan/virga/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Virga is a responsive Rust terminal weather application for current conditions,
multi-day forecasts, historical context, and hourly conditions visualization.
It is powered by Open-Meteo and requires no account or API key.

> **Project status: actively developed.** Virga works and is worth installing
> today. Features and fixes still land, so expect the occasional release.
> Contributions are welcome.

*Virga* is precipitation that evaporates before it reaches the ground. It is
also, most weeks, what the weathergram charts.

<img width="2000" height="1275" alt="CleanShot2026-08-12at19 00 55-ezgif com-speed" src="https://github.com/user-attachments/assets/0a773e11-df73-4cc3-9a75-f3bad3cbc727" />

## Contents

- [Hourly weathergram](#hourly-weathergram)
- [Highlights](#highlights)
- [Install](#install)
  - [Homebrew](#homebrew)
  - [Install script](#install-script)
  - [Download it yourself](#download-it-yourself)
  - [From source](#from-source)
  - [Updating and removing](#updating-and-removing)
- [Keys](#keys)
- [Commands](#commands)
- [Architecture](#architecture)
- [Engineering Quality](#engineering-quality)
- [Themes](#themes)
- [Where it starts](#where-it-starts)
- [Configuration](#configuration)
- [Contributing](#contributing)
  - [Cutting a release](#cutting-a-release)
- [Limitations](#limitations)
- [Data and Privacy](#data-and-privacy)
  - [Attribution](#attribution)
- [License](#license)

## Hourly weathergram

Press `p` for the hourly view. Sky, temperature, precipitation chance, and wind
share one clock, so a change in the forecast reads down a column instead of
across four separate tables. Temperature stands as a filled silhouette four
rows tall, the chance of rain rises in a band beneath it, a weather emoji
marks the sky every three hours, and the wind row carries an arrow every
second hour. The selected-hour pane gives the exact feels-like temperature,
humidity, rain or snow amount, wind and gusts, and the following 24-hour
precipitation total.

The visible horizon adapts through 12-, 24-, 36-, and 48-hour tiers as the
terminal widens. Arrow keys still browse the full eight-day hourly forecast,
and tall terminals retain the week-long precipitation strip below the
weathergram.

## Highlights

- **Current conditions** — temperature, feels-like, wind with gusts and
  direction, precipitation, daylight length, and US air quality index.
- **Eight-day forecast** — rain chance, maximum wind, UV index, sunrise, and
  sunset.
- **Three weeks of context** — fourteen days of historical highs followed by
  the current forecast, so today does not sit alone.
- **Hourly weathergram** — four shared-axis tracks, a selected-hour inspector,
  an adaptive horizon, and an optional weekly precipitation strip.
- **Fast navigation** — browse days or hours with the arrow keys, jump back to
  now, and inspect the selected period in detail.
- **Starts where you are** — the opening forecast is for the city your IP
  address resolves to, and a city you pick yourself replaces it permanently.
- **City search and live units** — search Open-Meteo's geocoder and switch
  between metric and imperial measurements without restarting.
- **A one-shot report** — `virga now` prints current conditions and today's
  outlook to stdout and exits, for a glance, a script, or a status bar.
- **Terminal-native presentation** — seven foreground-only themes, including
  one for light backgrounds, and responsive behavior down to a 34×12 terminal.

## Install

Virga needs a terminal with Unicode support and an internet connection. It does
not need Rust unless you are building it yourself.

### Homebrew

```bash
brew install t-shahan/tap/virga
```

macOS and Linux. This is the path that needs the least explanation on macOS,
because Homebrew does not apply the Gatekeeper quarantine that stops a binary
downloaded through a browser from opening.

### Install script

```bash
curl -fsSL https://raw.githubusercontent.com/t-shahan/virga/main/install.sh | sh
```

macOS and Linux. It picks the right build for your machine, checks the download
against the release's `SHA256SUMS` before installing anything, and puts the
binary in `~/.local/bin`. Two variables change what it does:

| Variable | Effect |
|---|---|
| `VIRGA_INSTALL_DIR` | Where the binary goes. Default `~/.local/bin` |
| `VIRGA_VERSION` | Install a specific tag, for example `v0.2.0` |

### Download it yourself

Every release on the [releases page][releases] carries a build for Linux
(x86_64 and aarch64, statically linked so distribution does not matter), macOS
(Apple silicon and Intel), and Windows (x86_64). Windows is not covered by the
install script, so the `.zip` is the way in.

Check what you downloaded against `SHA256SUMS`:

```bash
sha256sum --check --ignore-missing SHA256SUMS
```

Every binary also carries a build provenance attestation, which proves it was
produced by this repository's release workflow rather than by someone else:

```bash
gh attestation verify ./virga --repo t-shahan/virga
```

If you downloaded through a browser on macOS, Gatekeeper will refuse to open an
unsigned binary. Clear the quarantine flag:

```bash
xattr -dr com.apple.quarantine ./virga
```

### From source

Building requires **Rust 1.89 or later** — Virga uses `File::lock`, which
stabilized there, to keep concurrent state saves from losing each other's
half.

```bash
cargo install --git https://github.com/t-shahan/virga
```

The binary lands at `~/.cargo/bin/virga`. From a local checkout, use `cargo run
--release`; a plain `cargo run` produces an unoptimized build that is noticeably
slower to render.

### Updating and removing

Virga checks for a newer release once per launch, in the background, and
shows one muted line above the key bar when it finds one. The next keypress
on a screen that shows it clears it — letters typed into the city search
leave it standing, because that screen never shows it — and news that
arrived before you quit is printed on the way out instead. Set
`VIRGA_UPDATE=off` to skip the check; a launch that cannot reach GitHub — or
that quits before GitHub answers — simply shows no notice.

`virga update` asks the same question on demand and, judging from where the
running binary lives, says which row of this table applies to you. Neither
ever replaces the binary itself:

| Installed with | Update | Remove |
|---|---|---|
| Homebrew | `brew upgrade virga` | `brew uninstall virga` |
| Install script | Re-run the one-liner | `rm ~/.local/bin/virga` |
| Download | Download the new release | Delete the binary |
| Source | `cargo install --git https://github.com/t-shahan/virga --force` | `cargo uninstall virga-tui` |

Uninstalling from source takes the *package* name, `virga-tui`, not the binary
name.

Removing the binary leaves the one file Virga writes, a `state.json` holding
the last location you chose and, if you set one, your startup theme. It lives
in the platform's per-user state directory:

| Platform | Path |
|---|---|
| macOS | `~/Library/Application Support/com.t-shahan.virga/state.json` |
| Linux | `~/.local/state/virga/state.json` |
| Windows | `%LOCALAPPDATA%\t-shahan\virga\data\state.json` |

[releases]: https://github.com/t-shahan/virga/releases/latest

## Keys

| Key | Action |
|---|---|
| `←` `→` | Previous / next day — or hour, on the hourly screen; wraps |
| `↑` `↓` | Hourly screen: back / forward a day, keeping the time of day |
| `n` / `Home` | Jump back to now |
| `p` | Hourly weathergram — `b`, `Enter` or `Esc` to go back |
| `l` | Search for a city (`Enter` selects, `↑` `↓` move, `Esc` cancels) |
| `r` | Refetch the current location |
| `u` | Toggle metric / imperial |
| `t` | Cycle the colour theme — the key bar names the one you land on for a few seconds |
| `q` / `Esc` / `Ctrl-C` | Quit |

The hourly screen puts its four tracks on a shared axis. `▲` marks the selected
hour, a weather emoji marks the sky every three hours, and wind arrows every
second hour carry their speed on the six-hour ticks.

Choosing a city — or cancelling — returns to whichever screen the search was
opened from.

## Commands

No option changes how Virga runs, since the terminal is the whole interface.
What the command line answers, it answers without starting up.

| Command | What it does |
|---|---|
| `virga` | Start the application |
| `virga now` | Print current conditions and today's outlook for the remembered city |
| `virga now CITY` | The same for a searched city, e.g. `virga now paris`; multi-word names need no quotes |
| `virga theme` | List the themes, marking the startup default with `*` |
| `virga theme NAME` | Persist a startup theme, e.g. `virga theme tokyo night`; multi-word names need no quotes |
| `virga update` | Check whether a newer release exists and print how to get it |
| `virga help` / `-h` / `--help` | Print the usage text |
| `virga version` / `-V` / `--version` | Print the version |

`virga now` is the whole forecast reduced to a glance — the same sources, the
same city the app would open with, and none of the terminal:

```
$ virga now
Frederick, Maryland, United States · Partly cloudy
75°F, feels like 78°F · wind 7 mph · AQI 42 Good
Today: 84°F / 63°F · rain 20% · UV 6 · sun 06:24–20:07
```

Plain lines on stdout, so a script or a status bar reads it as easily as a
person does; a reading the provider did not report simply does not appear.
Set `VIRGA_UNITS=metric` to change what a degree is, here and on the app's
first frame. Asking about a named city is a question, not a move — the
remembered city stays whatever it was. And when nothing is remembered yet,
the one location lookup `virga now` makes is remembered afterwards, so a
status bar polling by the minute asks the location provider once, not once
per poll.

An unknown argument is an error rather than something to skip past. A typo
would otherwise start the application while the question behind it went
unanswered. Neither `virga update` nor the startup check replaces the binary; see
[Updating and removing](#updating-and-removing) for the command that does, and
[Configuration](#configuration) for the four environment variables.

## Architecture

Virga separates provider-specific data and network activity from application
state and terminal rendering:

```mermaid
flowchart LR
    keyboard["Keyboard events"] --> input["Event and input handling"]
    input --> state["Application state"]
    state --> ui["Ratatui UI"]

    api["Open-Meteo APIs"] --> client["HTTP client"]
    client --> dto["DTO conversion"]
    dto --> domain["Domain model"]
    domain --> state

    location["Remembered location"] <--> state
```

- [`src/ui/`](src/ui/) renders application state with Ratatui and performs no
  networking.
- [`src/weather/client.rs`](src/weather/client.rs) owns the weather,
  air-quality, and geocoding HTTP requests.
- [`src/weather/dto.rs`](src/weather/dto.rs) isolates Open-Meteo's wire formats
  and converts them into the stable domain data in
  [`src/weather/model.rs`](src/weather/model.rs).
- Event and input handling update application state, which coordinates
  navigation, search, refreshes, units, themes, and remembered location.

This boundary keeps provider changes out of the UI and makes both rendering and
API conversion independently testable.

## Engineering Quality

Virga's default locked test suite passes **387 deterministic tests**; four
provider-dependent live tests — three against Open-Meteo, one against
GitHub's release redirect — are ignored during normal runs.
Coverage includes:

- deterministic rendering checks built with Ratatui's `TestBackend`, including
  narrow and awkward terminal sizes;
- navigation wraparound, selection invariants, and metric/imperial conversion
  boundaries;
- null, malformed, truncated, and mismatched API-response handling;
- loopback-server tests proving refused and silent connections fail or time out
  instead of blocking the interface; and
- state persistence, input filtering, themes, charts, and stale asynchronous
  response handling.

GitHub Actions runs the locked suite on Linux, macOS, and Windows. Separate
gates enforce rustfmt, Clippy with warnings denied, the Rust 1.89 minimum
supported version, package-content completeness, and pinned dependency audits.
One more reads `CHANGELOG.md`: a section that has shipped may not be edited,
and a change a user can observe must describe itself under the section that
will carry it.

## Themes

`t` steps through seven palettes from either weather screen. The key bar names
the one you land on — `[t] theme (nord)` — and drops the name again a few
seconds later, so cycling tells you where you are without leaving a permanent
readout.

| Theme | Notes |
|---|---|
| `default` | The sixteen ANSI colours, so Virga looks the way your terminal is already configured to look |
| `gruvbox dark` | Warm throughout — orange bars, gold selection, green today |
| `nord` | Cool throughout — icy bars, aurora-purple selection |
| `tokyo night` | Blue and violet, with the selection the one warm thing on screen |
| `dracula` | The loud one — pink bars, lime selection, cyan today |
| `catppuccin mocha` | Pastel — mauve bars, sky selection, yellow today |
| `catppuccin latte` | The same scheme in dark ink, for terminals with a light background |

Every palette sets foregrounds only. None paints a background: your terminal's
own background remains visible, so a theme layers over your existing scheme
instead of stamping a separate dark rectangle over it. That also means a
palette has to suit the ground it lands on — `catppuccin latte` is the one
built for a light background, and the others assume a dark one.

The six non-default palettes use 24-bit colour. A terminal without truecolor
may approximate or ignore them, which is why `default` is the default: the
out-of-the-box appearance does not depend on truecolor support.

To make a theme the startup default, persist it:

```bash
virga theme tokyo night
```

`virga theme` alone lists the palettes and marks the one the next launch will
use. Names are case-insensitive and forgiving about separators, so `tokyo
night`, `tokyo-night`, and `Tokyo_Night` select the same theme, and multi-word
names need no quotes.

`VIRGA_THEME` still works and outranks the persisted theme for one launch:

```bash
VIRGA_THEME=gruvbox-dark virga
```

From weakest to strongest, the startup theme is the built-in `default`, the
persisted theme, `VIRGA_THEME`, and whatever `t` lands on — which lasts for
the session only, so cycling is a preview rather than a commitment. An
unrecognized `VIRGA_THEME` prints the known themes and falls back to the
persisted theme rather than refusing to run.

## Where it starts

Virga starts where your IP address says you are, so the first thing on screen is
your own weather rather than a city chosen for you.

Press `l` and pick somewhere, and that becomes the answer for good: a location
you chose is remembered in the platform's per-user state/data directory, and
every later launch starts there without asking the network anything. Detection
only runs while you have not chosen.

When the lookup does not answer — no network, or the service having a bad day —
Virga falls back to the last place it detected, and to New York City if it has
never detected one. The reason is printed on exit rather than shown as an error,
because a worse guess is not a reason to withhold the forecast.

Set `VIRGA_GEOIP=off` to skip the lookup entirely:

```bash
VIRGA_GEOIP=off virga
```

Startup is then the last location Virga knew, or New York City. An unrecognized
value prints a warning and leaves detection on rather than refusing to run.

## Configuration

`virga theme` persists the startup palette, `VIRGA_THEME` overrides it for one
launch, `VIRGA_GEOIP` turns location detection off, and `VIRGA_UPDATE` turns
the startup release check off, all as described above. `VIRGA_UNITS` picks
`metric` or `imperial` — `celsius`/`c` and `fahrenheit`/`f` also answer — for
the `virga now` report and the app's first frame alike; imperial when unset,
which is what the app has always started in. Unit and theme changes made in
the app last for the session.

Nothing else is configurable. The command line takes no options at all, only
the questions [Commands](#commands) lists.

## Contributing

Bug fixes, documentation improvements, tests, accessibility work, and
well-scoped features are welcome. Please open an issue before beginning a
substantial change so the approach and scope can be discussed. This is a
side project, so review timing varies, but pull requests do get read.

Before opening a pull request, run the same core checks used by CI:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

Notable changes belong in [`CHANGELOG.md`](CHANGELOG.md) under `Unreleased`.
That is not bookkeeping for its own sake: release notes are generated from that
file, and both the release script and CI refuse to publish a version it does not
describe.

### Cutting a release

```bash
./scripts/release.sh 0.3.0
```

It checks the working tree, bumps the manifest, runs the four gates above, then
commits, tags, and pushes. From there
[`.github/workflows/release.yml`](.github/workflows/release.yml) builds all five
platforms, publishes the release with checksums and build provenance, and
updates the Homebrew tap. The tap push authenticates with a `TAP_KEY` secret
holding an SSH private key whose public half is a write-enabled deploy key on
[the tap repository](https://github.com/t-shahan/homebrew-tap). A deploy key
rather than a personal access token, because it grants write to that one
repository and nothing else, and does not expire. Without the secret the release
still publishes and only the tap update is skipped, with a warning.

## Limitations

- There is no general configuration file, and weather is never cached. Every
  launch fetches fresh weather; only the startup theme and location are
  persisted, and units follow `VIRGA_UNITS` at startup and last for the
  session.
- Detection is city-level and sometimes wrong. Behind a VPN or a carrier-grade
  NAT it lands near your provider rather than near you — `l` fixes that
  permanently, and `VIRGA_GEOIP=off` avoids the lookup altogether.
- Forecast text is English only.
- Terminals below 34×12 show a size warning instead of the interface.
- “Today” is distinguished by colour alone in the daily chart. The selection
  is not: every screen marks it by shape as well — a `>` in the forecast
  table's gutter and a `▲` in the hourly weathergram.
- Ghostty and Apple's Terminal app have been tested manually on macOS.
- Automated tests run on Linux, macOS, and Windows. They do not validate
  real-terminal rendering, font fallback, or held-key behavior.

## Data and Privacy

Weather, air quality, and geocoding all come from
[Open-Meteo](https://open-meteo.com), which needs no API key. Its free tier is
**for non-commercial use only** and is rate limited to 10,000 calls per day.
Each weather load makes two requests, and each submitted search makes a third.
`virga now` asks the same questions the same way — two requests per report,
one more for a named city, and the location lookup only on a first run where
nothing is remembered.

Location detection is the one thing that does not go to Open-Meteo. On a launch
where you have not chosen a city, Virga makes a single request to
[ipapi.co](https://ipapi.co), which resolves the connection's own source address
to a city. Nothing else is sent — no query string, no identifier, no
coordinates, because working the coordinates out is the point of the request.
Their free tier needs no API key and allows 1,000 requests a day; Virga makes at
most one per launch. See ipapi.co's [privacy
policy](https://ipapi.co/privacy/) for what they retain.

That request is not made at all once you have chosen a city, or with
`VIRGA_GEOIP=off` set.

Checking for a newer release makes one request to GitHub's release-redirect
endpoint at github.com, carrying nothing but the request itself: no version
string, no identifier. The answer is read from a response header, and no
release page or file is downloaded. The check runs once per launch in the
background and whenever you run `virga update`; set `VIRGA_UPDATE=off` and
the launch-time request is not made at all.

Virga stores only the last successfully loaded location label and coordinates
locally, in its per-user state/data directory, alongside a note of whether you
chose it or it was detected — and, if you set one with `virga theme`, the name
of your startup theme. Your IP address is never written to disk — the resolved
city is. It does not store weather responses, searches, or history.
Weather and air-quality requests send the location coordinates to Open-Meteo;
city searches submit their search text to its geocoder. Open-Meteo's
free-service logs may retain IP addresses and coordinates for 90 days. See
Open-Meteo's [terms](https://open-meteo.com/en/terms) and
[licence](https://open-meteo.com/en/license).

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
the licensing of the data it fetches — see [Data and Privacy](#data-and-privacy).

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
