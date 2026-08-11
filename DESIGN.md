# virga — original design plan (historical)

> **This is a historical document, not a specification.**
>
> It is the plan written before any code existed, kept for context on why the
> stack was chosen. The app has since been built and has diverged from it: type
> shapes differ, module names differ (`events.rs`, not `event.rs`), and several
> decisions below were superseded — `serde_json` is now a dev-dependency only,
> and there is an hourly precipitation screen this plan never anticipated.
>
> **`README.md` and the source are authoritative.** Do not implement from this
> file or treat its signatures as current.

Architecture plan. Type shapes and signatures are given; **bodies are deliberately left as `todo!()`** — the implementation was the exercise.

## Decisions

| Decision | Choice | Why |
|---|---|---|
| TUI | `ratatui` + `crossterm` | The default Rust TUI stack; immediate-mode, so rendering is a pure function of state |
| HTTP | `ureq` (blocking) | No async runtime. Threads + channels teach ownership and `Send`; `tokio` would bury that under pinning and future lifetimes |
| Concurrency | `std::thread` + `std::sync::mpsc` | Enough for one background fetcher |
| JSON | `serde` + `serde_json` | Derive macros are the idiomatic path |
| Errors | `anyhow::Result` in app/UI code | Avoids drowning in `From` impls early. Swap the `weather` module to a hand-rolled `enum` error later — that's a good standalone lesson |
| Weather API | **Open-Meteo** | No API key, so no secrets handling at all. Forecast + geocoding both keyless |

```bash
cargo add ratatui ureq serde serde_json anyhow
cargo add serde --features derive
cargo add ureq --features json
```

Two gotchas:

- **Do not add `crossterm` as a separate dependency.** `ratatui` re-exports it as `ratatui::crossterm`. A separately-versioned `crossterm` gives you two incompatible `Event` types and error messages that make no sense.
- Open-Meteo's query parameters have changed across revisions. Read the current docs for `current=` / `daily=` field names rather than trusting any example, including mine.

## Module layout

```
src/
├── main.rs          terminal setup/teardown, panic hook, run loop
├── app.rs           App state + state transitions        (no I/O, no ratatui)
├── event.rs         input events and worker messages
├── weather/
│   ├── mod.rs
│   ├── client.rs    HTTP calls                            (the only network code)
│   ├── dto.rs       serde structs mirroring the JSON wire format
│   ├── model.rs     domain types the UI actually renders
│   └── code.rs      WMO weather code → description/icon
└── ui/
    ├── mod.rs       draw(frame, &App) — top-level layout
    ├── current.rs
    ├── forecast.rs
    └── search.rs
```

The load-bearing rule: **`app.rs`, `weather/model.rs`, and `weather/code.rs` import neither `ratatui` nor `ureq`.** That's what keeps the logic unit-testable, since a TUI can't be verified from test output. `ui/` reads `&App` and never mutates it.

`dto.rs` and `model.rs` being separate is a real judgment call. The payoff: the wire format's flat parallel arrays (`time[]`, `temperature_max[]`) are hostile to render against, and the seam gives you a natural `TryFrom<ForecastDto> for Forecast`. If it feels like ceremony on day one, collapse them and split later when the shapes diverge.

## Core types

The single most valuable thing here — model loading and failure as an **enum**, not `is_loading: bool` + `error: Option<String>`. Illegal states stop compiling.

```rust
// app.rs
pub enum Fetch<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

pub enum Screen {
    Search,
    Weather,
}

pub struct App {
    pub screen: Screen,
    pub query: String,               // search input buffer
    pub location: Option<Location>,
    pub weather: Fetch<Weather>,
    pub units: Units,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self { todo!() }
    pub fn on_key(&mut self, key: KeyEvent) -> Option<Request> { todo!() }
    pub fn on_message(&mut self, msg: Message) { todo!() }
}
```

`on_key` returning `Option<Request>` is the key move: `App` never performs I/O, it only *asks* for it. That keeps it testable — feed keys, assert on state — and the run loop stays a dumb pump.

```rust
// weather/model.rs
pub struct Location { pub name: String, pub country: String, pub lat: f64, pub lon: f64 }
pub struct Weather  { pub current: Current, pub daily: Vec<DailyForecast> }
pub struct Current  { pub temp_c: f64, pub feels_like_c: f64, pub code: WeatherCode, pub wind_kph: f64 }
pub struct DailyForecast { pub date: NaiveDate, pub high_c: f64, pub low_c: f64, pub code: WeatherCode }
```

Store canonical units (Celsius) and convert at render time. `Units` is a display concern; if it leaks into the model you'll be converting back and forth forever.

## Threading

Main thread owns the terminal and `App`. One worker thread owns the network.

```
        Request                    Message
App ───────────────► worker ──────────────────► App
   (mpsc::Sender)            (mpsc::Sender)
```

```rust
// event.rs
pub enum Request { Search(String), Fetch(Location) }
pub enum Message { Located(Vec<Location>), Loaded(Weather), Failed(String) }
```

The run loop, once per iteration:

1. `event::poll(Duration::from_millis(100))?` — non-blocking-ish input read
2. drain `rx.try_recv()` for worker messages → `app.on_message(..)`
3. `terminal.draw(|f| ui::draw(f, &app))?`

The 100 ms poll timeout is what keeps the UI responsive while the worker is busy: input never blocks on the network, so the spinner actually spins.

A panic in raw mode would normally leave the terminal unusable, since unwinding skips `ratatui::restore()`. **ratatui 0.30 handles this for you** — `try_init()` calls `set_panic_hook()` before enabling raw mode, so no hook of your own is needed. If you ever install your own panic hook, install it *before* `ratatui::init()` so ratatui's restore runs first.

## Build order

Each milestone compiles and runs. Network comes late on purpose — you get something on screen fast, and each step introduces one new Rust concept.

| # | Milestone | New concept |
|---|---|---|
| 0 | Raw mode, alt screen, a bordered box, `q`/`Esc`/Ctrl-C quits | Pattern matching, match guards, `?` |
| 1 | Full UI rendered against a **hardcoded** `Weather` fixture | Borrowing, layout, struct design |
| 2 | Parse a saved JSON file → DTO → domain, unit tested | `serde` derive, `TryFrom`, iterators |
| 3 | Real HTTP **on the main thread** — the UI will freeze during fetch | Traits, `?` across error types |
| 4 | Move fetch to a worker thread; add `Fetch<T>` states + spinner | `Send`, `'static`, channels, ownership across threads |
| 5 | Search screen: text input, results list, selection | Enums for state, `&mut` discipline |
| 6 | Units toggle, error surface, cache last result | Whatever you want |

Step 3 freezing the UI is intentional. Feeling that freeze is what makes step 4 obvious rather than abstract.

Save a real API response to `tests/fixtures/forecast.json` during step 2 — it becomes your parser's regression test and lets you work offline.

## Explicitly out of scope

No config file, no plugin traits, no async, no custom `Error` enum, no caching layer until step 6. Each one is a fine exercise later; none of them earns its complexity before the thing renders weather.
