# CLAUDE.md

Guidance for Claude Code (claude.ai/code) working in this repository.

## What this is

`virga` is a finished, working terminal weather app: current conditions, an
eight-day forecast, three weeks of daily highs, and an hourly precipitation
screen. Package name `virga-tui` (the bare `virga` is taken on crates.io by an
unrelated crate); binary name `virga`. Licensed GPL-3.0-or-later.

It is being prepared for an open-source release. An audit and a feature plan
live *outside* the repository at `../AUDIT.md` and `../PRECIPITATION_PLAN.md`;
the audit's remaining open items are listed under [Known gaps](#known-gaps).

## Commands

```bash
cargo run --release        # the app; plain `cargo run` renders noticeably slower
cargo build --release      # → target/release/virga
cargo check                # fast type/borrow check
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test                 # 155 deterministic tests
cargo test -- --ignored    # 2 live-API tests, excluded by default
cargo test <name> -- --exact --nocapture
cargo +1.88.0 test --locked --all-targets   # the declared MSRV
```

## Toolchain

`rust-version = "1.88"`. Edition 2024 only requires 1.85, but `ratatui 0.30.2`
and `ratatui-core 0.1.2` both declare 1.88.0 — the dependency graph's floor is
what binds. There is no `rust-toolchain.toml`, so local builds use whatever is
on `PATH`; verify the minimum explicitly with `cargo +1.88.0`.

## Architecture

```
src/
├── main.rs      terminal setup, the event loop, key routing
├── app.rs       App state, Fetch<T>, Screen, ActiveLocation, navigation
├── events.rs    the worker thread; Request / Message
├── units.rs     metric/imperial conversion and labels
├── weather/
│   ├── client.rs  HTTP; one shared, timeout-bounded ureq Agent
│   ├── dto.rs     wire types and their conversion into the domain model
│   ├── model.rs   Weather, DailyForecast, HourlyForecast, Current, AirQuality
│   └── code.rs    WMO code → description / emoji / AQI label
└── ui/
    ├── mod.rs          render dispatch, MIN_WIDTH/HEIGHT, shared border helpers
    ├── current.rs      the top pane on the weather screen
    ├── forecast.rs     the forecast table
    ├── chart.rs        the daily bar chart
    ├── bars.rs         column geometry shared by both charts
    ├── precip.rs       the precipitation screen: layout, detail pane, next-rain
    ├── precip_chart.rs the mirrored hourly chart (writes cells directly)
    ├── digits.rs       the block-digit font
    ├── search.rs       the city-search overlay
    └── legend.rs       the keybind bar
```

One blocking worker thread does all I/O. `main.rs` sends `Request` down an mpsc
channel and drains `Message` back each tick; the UI never blocks on the network.

## Invariants worth preserving

These are load-bearing. Several were bugs before they were rules.

- **`dto.rs` knows about `model.rs`, never the reverse.** Wire shape and domain
  shape are deliberately separate.
- **`ui/` does no networking; `weather/` does no rendering.**
- **Every API field is `Option`.** A missing reading degrades to a dash or is
  omitted — it never fails the fetch. See `at()` / `at_owned()` in `dto.rs`.
- **Layout thresholds are derived from constants, never hardcoded.** See
  `SIDE_BY_SIDE_MIN` in `ui/mod.rs` and `Columns::fit` in `ui/bars.rs`. This has
  been a repeated source of bugs.
- **The label and coordinates of a location travel together** as
  `ActiveLocation`, and only a successful load commits one. Splitting them is
  what made refresh silently fetch the wrong city.
- `Alignment::Center` centres **each line on its own width**. To centre a block,
  centre the *area* with `Flex::Center` and leave the text left-aligned.
- A missing amount is not a zero amount. Positive precipitation that rounds to
  zero at the display precision renders `<0.01 in`, never `0.00 in`.

## Testing

A TUI cannot be verified by reading assertions alone. Two rules follow:

1. **Render to a `TestBackend` and assert against the buffer**, not against the
   arithmetic that produced it. Several bugs here survived rounds of
   plausible-sounding fixes and died the moment someone printed the buffer.
2. **Exercise anything interactive in a real terminal.** Key routing lives
   inline in `run()` and has no test coverage at all — the mapping from
   `KeyCode` to an `App` method is verified only by running the binary.

Tests target what breaks quietly: layout at awkward sizes (including the 34×12
minimum), null and mismatched-length API responses, unit-conversion boundaries,
column fit in **both** unit systems, navigation at the wrap points, and HTTP
timeouts against a loopback server that never answers.

Fixtures that are a whole number of days hid a real day-stepping bug, because
the live window is a whole number of days only by coincidence. Prefer awkward
fixture sizes.

## Known gaps

Open items from `../AUDIT.md`, roughly in priority order:

- **§1.2** — an in-flight search response is still accepted after the query
  changes. `ActiveLocation` and commit-on-response built most of the seam this
  needs; it wants request IDs.
- **§2.1 / §7.3** — key release and repeat events are not filtered, so Windows
  input can duplicate and a held key can flood the request queue.
- **§2.2** — key handling and state transitions are inline in `run()`, so none
  of it is testable. Extracting a terminal-independent `Action` enum plus
  `App::on_action` / `App::on_message` is the prerequisite for §1.2 and §2.1.
- **§2.3 / §7.6** — no CI. macOS is the only platform ever tested.
- **§2.6** — today's AQI is a current reading and other days' is a daily
  maximum, both rendered under the same `AQI` label.
- **§7.11** — the forecast table and daily chart still encode selection in
  colour alone.

## Documents

`DESIGN.md` is the **original plan**, kept for history. It describes types that
were never built and bodies left as `todo!()`. The source and `README.md` are
authoritative; do not treat `DESIGN.md` as a specification.
