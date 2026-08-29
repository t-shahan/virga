# Hourly Weathergram Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Virga's precipitation-only hourly screen with an adaptive, shared-axis weathergram for sky, temperature, precipitation probability, and wind while preserving its exact selected-hour detail and weekly precipitation context.

**Architecture:** Extend the existing Open-Meteo hourly DTO and domain model without adding a request, then keep selection and navigation in `App` while deriving all windowing and scaling inside pure UI functions. Rename the existing screen route to `Hourly`, replace the precipitation renderer with a focused inspector plus weathergram renderer, and retain `precip_week` as the height-dependent long-range layer.

**Tech Stack:** Rust 1.88, Ratatui 0.30.2, ureq 3.3, serde/serde_json, chrono, Cargo's built-in test harness

**Spec:** `docs/superpowers/specs/2026-08-26-hourly-weathergram-design.md`

## Global Constraints

- Rust 1.88 remains the minimum supported version.
- Add no dependency; use the versions already locked in `Cargo.lock`.
- Open-Meteo remains the only weather-data provider, and a weather load remains exactly two network requests.
- Keep `p` as the opening and closing shortcut, but name the destination `Hourly` and label it `hourly` in the legend.
- Support the existing 34×12 minimum terminal; below it, retain the current size warning.
- Treat every new hourly measurement as independently optional; missing values must not drop or shift an hour.
- Do not persist weathergram window, horizon, scale, or track state.
- Preserve all five foreground-only themes and never use color as the sole selection/current-hour marker.
- Before handoff, pass `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked --all-targets`, and `cargo package --locked`.

## File Structure

- Create `src/ui/condition_symbol.rs`: one-cell WMO condition vocabulary only.
- Create `src/ui/weathergram.rs`: adaptive horizon, visible window, track calculations, summaries, markers, and shared-axis rendering.
- Move `src/ui/precip.rs` to `src/ui/hourly.rs`: selected-hour inspector and responsive composition of inspector, weathergram, and weekly strip.
- Delete `src/ui/precip_chart.rs` after its required probability, amount-detail, selection, and responsive coverage has moved.
- Modify `src/weather/model.rs`, `src/weather/dto.rs`, and `src/weather/client.rs`: new optional hourly readings in the existing request path.
- Modify `src/app.rs`, `src/input.rs`, `src/ui/mod.rs`, and `src/ui/legend.rs`: semantic screen/action rename and renderer routing.
- Modify `src/ui/axis.rs`, `src/ui/bars.rs`, and `src/ui/precip_week.rs` only where shared comments, fixtures, or interfaces must reflect the weathergram.
- Modify `README.md` and `Cargo.toml`: product copy, key descriptions, and removal of the obsolete precipitation-chart screenshot.

---

### Task 1: Extend the hourly weather data contract

**Files:**
- Modify: `src/weather/model.rs:58-72,126-175`
- Modify: `src/weather/dto.rs:152-170,237-252,528-656`
- Modify: `src/weather/client.rs:34-39,134-160,580-615`
- Modify: `src/ui/precip.rs:432-442` (temporary fixture; this file moves in Task 5)
- Modify: `src/ui/precip_week.rs:369-387`
- Test: inline `#[cfg(test)]` modules in the files above

**Interfaces:**
- Consumes: Open-Meteo hourly arrays named `apparent_temperature`, `relative_humidity_2m`, `wind_speed_10m`, `wind_gusts_10m`, and `wind_direction_10m`.
- Produces: `HourlyForecast { feels_like_c, humidity_pct, wind_kph, gust_kph, wind_dir_deg }`, each as an independent `Option` used by Tasks 4 and 5.
- Produces: `const HOURLY_FIELDS: &str` in `weather/client.rs`, used directly by the existing forecast request.

Provider reference: [Open-Meteo Forecast API hourly parameter definitions](https://open-meteo.com/en/docs#hourly_parameter_definition).

- [ ] **Step 1: Add a failing DTO conversion test for the five readings**

Extend `FOUR_HOURS` in `src/weather/dto.rs` with the five arrays, then add this test:

```rust
#[test]
fn hourly_conditions_are_mapped_by_timestamp_index() {
    let weather = with_hourly(FOUR_HOURS);
    let hour = &weather.hourly[1];

    assert_eq!(hour.time, "2026-08-09T01:00");
    assert_eq!(hour.feels_like_c, Some(16.5));
    assert_eq!(hour.humidity_pct, Some(60));
    assert_eq!(hour.wind_kph, Some(10.0));
    assert_eq!(hour.gust_kph, Some(15.0));
    assert_eq!(hour.wind_dir_deg, Some(90.0));
}
```

Use these exact fixture arrays:

```json
"apparent_temperature": [17.0, 16.5, null, 15.0],
"relative_humidity_2m": [55, 60, null, 70],
"wind_speed_10m": [5.0, 10.0, null, 20.0],
"wind_gusts_10m": [8.0, 15.0, null, 30.0],
"wind_direction_10m": [0.0, 90.0, null, 225.0]
```

- [ ] **Step 2: Run the DTO test and verify it fails to compile on the new fields**

Run: `cargo test --locked hourly_conditions_are_mapped_by_timestamp_index`

Expected: FAIL with errors that `HourlyForecast` has no fields named `feels_like_c`, `humidity_pct`, `wind_kph`, `gust_kph`, and `wind_dir_deg`.

- [ ] **Step 3: Add the optional fields from DTO through domain conversion**

Add to `HourlyForecast`:

```rust
pub feels_like_c: Option<f64>,
pub humidity_pct: Option<u8>,
pub wind_kph: Option<f64>,
pub gust_kph: Option<f64>,
pub wind_dir_deg: Option<f64>,
```

Add defaulted arrays to `HourlyDto`:

```rust
#[serde(default)]
pub apparent_temperature: Vec<Option<f64>>,
#[serde(default)]
pub relative_humidity_2m: Vec<Option<u8>>,
#[serde(default)]
pub wind_speed_10m: Vec<Option<f64>>,
#[serde(default)]
pub wind_gusts_10m: Vec<Option<f64>>,
#[serde(default)]
pub wind_direction_10m: Vec<Option<f64>>,
```

Map them by the same index as `time`:

```rust
feels_like_c: at(&hour.apparent_temperature, i),
humidity_pct: at(&hour.relative_humidity_2m, i),
wind_kph: at(&hour.wind_speed_10m, i),
gust_kph: at(&hour.wind_gusts_10m, i),
wind_dir_deg: at(&hour.wind_direction_10m, i),
```

Give every test constructor deterministic values. In `Weather::fixture`, use
`Some(14.0 + (i % 12) as f64)` for feels-like, `Some(55 + (i % 20) as u8)`
for humidity, `Some(8.0 + (i % 10) as f64)` for wind, `Some(15.0 + (i % 12)
as f64)` for gusts, and `Some(((i * 45) % 360) as f64)` for direction. In local
dry-hour helpers, use fixed `Some(19.0)`, `Some(55)`, `Some(10.0)`,
`Some(18.0)`, and `Some(225.0)` respectively.

- [ ] **Step 4: Prove absent and short new arrays remain field-level absence**

Extend the existing absent-hourly and mismatched-array tests with assertions like:

```rust
let old_fixture = with_hourly(r#"{
    "time": ["2026-08-09T00:00"],
    "temperature_2m": [18.0]
}"#);
assert_eq!(old_fixture.hourly.len(), 1);
assert!(old_fixture.hourly[0].humidity_pct.is_none());
assert!(old_fixture.hourly[0].wind_kph.is_none());

let weather = with_hourly(r#"{
    "time": ["2026-08-09T00:00", "2026-08-09T01:00"],
    "relative_humidity_2m": [55]
}"#);
assert_eq!(weather.hourly.len(), 2);
assert_eq!(weather.hourly[0].humidity_pct, Some(55));
assert_eq!(weather.hourly[1].humidity_pct, None);
```

- [ ] **Step 5: Run DTO and fixture tests**

Run: `cargo test --locked weather::dto::tests && cargo test --locked ui::precip_week::tests`

Expected: PASS; existing `tests/fixtures/forecast.json` remains unchanged and therefore proves old responses still parse.

- [ ] **Step 6: Add a failing client contract test for the hourly query list**

Declare the expected list in the client test before defining the production constant:

```rust
#[test]
fn hourly_request_names_every_weathergram_field() {
    assert_eq!(
        HOURLY_FIELDS.split(',').collect::<Vec<_>>(),
        vec![
            "precipitation",
            "precipitation_probability",
            "snowfall",
            "weather_code",
            "temperature_2m",
            "apparent_temperature",
            "relative_humidity_2m",
            "wind_speed_10m",
            "wind_gusts_10m",
            "wind_direction_10m",
        ]
    );
}
```

- [ ] **Step 7: Run the client test and verify it fails**

Run: `cargo test --locked hourly_request_names_every_weathergram_field`

Expected: FAIL because `HOURLY_FIELDS` is not defined.

- [ ] **Step 8: Define and use the exact hourly field list**

Add near the timeout constants:

```rust
const HOURLY_FIELDS: &str = "precipitation,precipitation_probability,snowfall,weather_code,temperature_2m,apparent_temperature,relative_humidity_2m,wind_speed_10m,wind_gusts_10m,wind_direction_10m";
```

Replace the inline `hourly` query value with:

```rust
.query("hourly", HOURLY_FIELDS)
```

Extend the ignored live hourly smoke test so it asserts at least one forward
hour has each new reading; keep it ignored in normal CI.

- [ ] **Step 9: Run the task verification**

Run: `cargo fmt --check && cargo test --locked weather::dto::tests && cargo test --locked weather::client::tests && cargo test --locked ui::precip_week::tests`

Expected: all commands PASS.

- [ ] **Step 10: Commit the data contract**

```bash
git add src/weather/model.rs src/weather/dto.rs src/weather/client.rs src/ui/precip.rs src/ui/precip_week.rs
git commit -m "feat: load full hourly conditions"
```

---

### Task 2: Rename the screen route without changing behavior

**Files:**
- Modify: `src/app.rs:17-21,251-275,758-776,1800-1850`
- Modify: `src/input.rs:11-37,90-138,141-365`
- Modify: `src/ui/mod.rs:95-168,249-628`
- Modify: `src/ui/legend.rs:41-80,190-211,300-390`
- Modify: `src/ui/precip.rs:417-840` (temporary module name only)

**Interfaces:**
- Produces: `Screen::Hourly` replacing `Screen::Precipitation` everywhere.
- Produces: `Action::OpenHourly` replacing `Action::OpenPrecipitation` everywhere.
- Preserves: `p` opens Hourly from Weather and acts as Back from Hourly.
- Preserves: the existing `precip_render` entry point until Task 5 replaces it.

- [ ] **Step 1: Rename the input tests first**

Change the focused shortcut test to express the approved contract:

```rust
#[test]
fn p_both_opens_and_closes_the_hourly_screen() {
    assert_eq!(
        action_for(press(KeyCode::Char('p')), Screen::Weather),
        Some(Action::OpenHourly)
    );
    assert_eq!(
        action_for(press(KeyCode::Char('p')), Screen::Hourly),
        Some(Action::Back)
    );
}
```

Update the legend test to require `[p] hourly` and the hourly-screen arrow/back labels.

- [ ] **Step 2: Run the focused tests and verify the semantic names fail**

Run: `cargo test --locked p_both_opens_and_closes_the_hourly_screen && cargo test --locked the_weather_legend_advertises_the_hourly_screen`

Expected: FAIL to compile because `Screen::Hourly` and `Action::OpenHourly` do not exist.

- [ ] **Step 3: Rename the enum variants and every typed reference**

Use these enum declarations:

```rust
pub enum Screen {
    Weather,
    Search,
    Hourly,
}
```

```rust
pub enum Action {
    Quit,
    Back,
    Refresh,
    ToggleUnits,
    CycleTheme,
    OpenSearch,
    OpenHourly,
    PrevDay,
    NextDay,
    Today,
    PrevHour,
    NextHour,
    PrevHourDay,
    NextHourDay,
    Now,
    Insert(char),
    Backspace,
    Submit,
    PrevResult,
    NextResult,
}
```

In `App::on_action`, retain the reset-to-now behavior:

```rust
Action::OpenHourly => {
    self.screen = Screen::Hourly;
    self.select_now();
}
```

Update all app, input, legend, UI state-sweep, search-return, theme, and
temporary precipitation-renderer tests to the new typed names. Change the
weather legend pair to `("p", "hourly")`; do not add an `h` alias.

- [ ] **Step 4: Verify no old typed route remains**

Run: `rg -n 'Screen::Precipitation|OpenPrecipitation' src`

Expected: no output. Module names such as `precip_render` are allowed until Task 5.

- [ ] **Step 5: Run route and UI tests**

Run: `cargo test --locked app::tests && cargo test --locked input::tests && cargo test --locked ui::legend::tests && cargo test --locked ui::tests`

Expected: PASS with rendering behavior unchanged.

- [ ] **Step 6: Commit the semantic rename**

```bash
git add src/app.rs src/input.rs src/ui/mod.rs src/ui/legend.rs src/ui/precip.rs
git commit -m "feat: rename precipitation screen to hourly"
```

---

### Task 3: Add the one-cell condition vocabulary

**Files:**
- Create: `src/ui/condition_symbol.rs`
- Modify: `src/ui/mod.rs:9-20`
- Test: inline `src/ui/condition_symbol.rs` test module

**Interfaces:**
- Consumes: `Option<u8>` WMO weather code from `HourlyForecast::code`.
- Produces: `pub(super) fn symbol(code: Option<u8>) -> &'static str`.
- Invariant: `None` is one blank cell, recognized and unknown reported codes are exactly one visible terminal cell.

- [ ] **Step 1: Register the module and write the failing mapping tests**

Add `mod condition_symbol;` in `src/ui/mod.rs`, create the file, and start with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Line;

    #[test]
    fn groups_documented_wmo_codes_into_one_cell_conditions() {
        for (codes, expected) in [
            (&[0][..], "○"),
            (&[1, 2], "◐"),
            (&[3], "●"),
            (&[45, 48], "≡"),
            (&[51, 53, 55, 56, 57], "┆"),
            (&[61, 63, 65, 66, 67, 80, 81, 82], "│"),
            (&[71, 73, 75, 77, 85, 86], "*"),
            (&[95, 96, 99], "ϟ"),
        ] {
            for code in codes {
                assert_eq!(symbol(Some(*code)), expected, "code {code}");
            }
        }
    }

    #[test]
    fn absence_is_blank_and_an_unknown_reported_code_is_a_question() {
        assert_eq!(symbol(None), " ");
        assert_eq!(symbol(Some(200)), "?");
    }

    #[test]
    fn every_drawn_symbol_occupies_one_terminal_cell() {
        for code in [0, 1, 3, 45, 51, 61, 71, 95, 200] {
            assert_eq!(Line::from(symbol(Some(code))).width(), 1, "code {code}");
        }
    }
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test --locked ui::condition_symbol::tests`

Expected: FAIL because `symbol` is undefined.

- [ ] **Step 3: Implement the complete mapping**

```rust
pub(super) fn symbol(code: Option<u8>) -> &'static str {
    match code {
        None => " ",
        Some(0) => "○",
        Some(1 | 2) => "◐",
        Some(3) => "●",
        Some(45 | 48) => "≡",
        Some(51 | 53 | 55 | 56 | 57) => "┆",
        Some(61 | 63 | 65 | 66 | 67 | 80 | 81 | 82) => "│",
        Some(71 | 73 | 75 | 77 | 85 | 86) => "*",
        Some(95 | 96 | 99) => "ϟ",
        Some(_) => "?",
    }
}
```

- [ ] **Step 4: Run and commit the symbol module**

Run: `cargo fmt --check && cargo test --locked ui::condition_symbol::tests`

Expected: PASS.

```bash
git add src/ui/mod.rs src/ui/condition_symbol.rs
git commit -m "feat: add hourly condition symbols"
```

---

### Task 4: Build the adaptive weathergram renderer

**Files:**
- Create: `src/ui/weathergram.rs`
- Modify: `src/ui/mod.rs:9-20`
- Reuse: `src/ui/axis.rs`
- Reuse: `src/ui/bars.rs`
- Test: inline `src/ui/weathergram.rs` test module

**Interfaces:**
- Consumes: `&[HourlyForecast]`, `Palette`, `Unit`, selected forward-hour index, `Rect`, and `compact: bool`.
- Consumes: `condition_symbol::symbol`, `bars::window_start`, and axis cell/tick helpers.
- Produces: `pub(super) const FULL_ROWS: u16 = 8` and `pub(super) const COMPACT_ROWS: u16 = 6`.
- Produces: `pub(super) fn weathergram_render(frame: &mut Frame, hours: &[HourlyForecast], palette: Palette, area: Rect, unit: Unit, selected: usize, compact: bool)`.
- Keeps private: `Window { start, hours, cell_width }`, scale and summary helpers.

- [ ] **Step 1: Register the module and write failing horizon/window tests**

Create `src/ui/weathergram.rs` with the private result type and these tests:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Window {
    start: usize,
    hours: usize,
    cell_width: u16,
}

#[test]
fn horizons_are_quantized_from_measured_plot_width() {
    assert_eq!(window_for(34, 0, 192).hours, 12);
    assert_eq!(window_for(80, 0, 192).hours, 24);
    assert_eq!(window_for(100, 0, 192).hours, 36);
    assert_eq!(window_for(120, 0, 192).hours, 48);
}

#[test]
fn a_selection_moves_inside_a_stable_page() {
    for selected in 0..24 {
        assert_eq!(window_for(80, selected, 192).start, 0);
    }
    assert_eq!(window_for(80, 24, 192).start, 24);
    assert_eq!(window_for(80, 191, 192).start, 168);
}
```

- [ ] **Step 2: Run the window tests and verify they fail**

Run: `cargo test --locked ui::weathergram::tests::horizons_are_quantized_from_measured_plot_width`

Expected: FAIL because `window_for` is undefined.

- [ ] **Step 3: Implement width accounting and quantized window selection**

Use these constants and calculation:

```rust
pub(super) const FULL_ROWS: u16 = 8;
pub(super) const COMPACT_ROWS: u16 = 6;

const BORDER_COLS: u16 = 2;
const LABEL_WIDTH: u16 = 6;
const SUMMARY_GAP: u16 = 1;
const SUMMARY_WIDTH: u16 = 12;
const HORIZONS: [usize; 3] = [48, 36, 24];
const COMPACT_HORIZON: usize = 12;
const MAX_CELL_WIDTH: u16 = 3;

fn window_for(width: u16, selected: usize, count: usize) -> Window {
    let plot_width = width.saturating_sub(
        BORDER_COLS + LABEL_WIDTH + SUMMARY_GAP + SUMMARY_WIDTH,
    );
    let horizon = HORIZONS
        .into_iter()
        .find(|hours| plot_width as usize >= hours * 2)
        .unwrap_or(COMPACT_HORIZON);
    let hours = if count == 0 { horizon } else { horizon.min(count) };
    let cell_width = (plot_width / hours as u16).clamp(1, MAX_CELL_WIDTH);

    Window {
        start: window_start(selected.min(count.saturating_sub(1)), hours, count),
        hours,
        cell_width,
    }
}
```

Keep `hours` as the selected horizon when `count == 0` only for layout; slice
the input with checked ranges so an empty forecast draws labels and blanks
without indexing.

- [ ] **Step 4: Add failing pure track tests**

```rust
#[test]
fn a_flat_temperature_range_uses_the_middle_step() {
    assert_eq!(temperature_step(Some(12.0), Some((12.0, 12.0))), "▄");
    assert_eq!(temperature_step(None, Some((12.0, 12.0))), " ");
}

#[test]
fn rain_probability_uses_the_week_strip_thresholds() {
    for (chance, expected) in [
        (None, " "),
        (Some(0), "·"),
        (Some(9), "·"),
        (Some(10), "▂"),
        (Some(30), "▄"),
        (Some(50), "▆"),
        (Some(70), "█"),
        (Some(100), "█"),
    ] {
        assert_eq!(rain_step(chance), expected);
    }
}

#[test]
fn wind_uses_eight_sectors_and_a_calm_dot() {
    assert_eq!(wind_symbol(Some(0.5), None), "·");
    assert_eq!(wind_symbol(Some(10.0), Some(0.0)), "↑");
    assert_eq!(wind_symbol(Some(10.0), Some(45.0)), "↗");
    assert_eq!(wind_symbol(Some(10.0), Some(90.0)), "→");
    assert_eq!(wind_symbol(Some(10.0), Some(225.0)), "↙");
    assert_eq!(wind_symbol(Some(10.0), Some(-45.0)), "↖");
    assert_eq!(wind_symbol(Some(10.0), None), " ");
}
```

- [ ] **Step 5: Implement track values and summaries**

Use `▁`, `▂`, `▄`, `▆`, `█` for temperature quintiles. Ignore missing values
when finding the visible min/max; a flat range returns `▄`; absence returns a
blank. Implement the fixed probability mapping exactly as the test states.

Normalize direction before choosing a sector:

```rust
fn wind_symbol(speed_kph: Option<f64>, direction: Option<f64>) -> &'static str {
    const ARROWS: [&str; 8] = ["↑", "↗", "→", "↘", "↓", "↙", "←", "↖"];
    if speed_kph.is_some_and(|speed| speed < 1.0) {
        return "·";
    }
    let Some(degrees) = direction else {
        return " ";
    };
    let normalized = degrees.rem_euclid(360.0);
    ARROWS[((normalized / 45.0).round() as usize) % ARROWS.len()]
}
```

Add helpers with these signatures:

```rust
fn temperature_range(hours: &[HourlyForecast]) -> Option<(f64, f64)>;
fn temperature_step(value: Option<f64>, range: Option<(f64, f64)>) -> &'static str;
fn rain_step(chance: Option<u8>) -> &'static str;
fn wind_symbol(speed_kph: Option<f64>, direction: Option<f64>) -> &'static str;
fn temperature_summary(hours: &[HourlyForecast], unit: Unit) -> String;
fn precipitation_summary(hours: &[HourlyForecast], selected: usize, unit: Unit) -> String;
fn wind_summary(hours: &[HourlyForecast], unit: Unit) -> String;
```

`precipitation_summary` sums `precip_mm` over the 24 hours beginning at the
selection, clips at the end of the series, and formats with
`Unit::precip_decimals()` and `Unit::precip_label()`.

- [ ] **Step 6: Run pure calculation tests**

Run: `cargo test --locked ui::weathergram::tests`

Expected: all pure tests PASS; renderer tests are added next.

- [ ] **Step 7: Add failing full and compact renderer tests**

Use `Weather::fixture(22, 14).forecast_hours()` with `TestBackend` and assert:

```rust
#[test]
fn full_weathergram_draws_four_aligned_tracks_and_markers() {
    let weather = Weather::fixture(22, 14);
    let text = rendered(&weather, 80, FULL_ROWS, 3, false);

    for label in ["sky", "temp", "rain", "wind"] {
        assert!(text.contains(label), "missing {label}:\n{text}");
    }
    assert!(text.contains('▲'), "selection has no shape marker:\n{text}");
    assert!(text.contains("next 24 h"), "wrong horizon:\n{text}");
}

#[test]
fn compact_weathergram_keeps_every_track_in_six_rows() {
    let weather = Weather::fixture(22, 14);
    let text = rendered(&weather, 34, COMPACT_ROWS, 0, true);

    assert_eq!(text.lines().count(), COMPACT_ROWS as usize);
    for label in ["sky", "temp", "rain", "wind"] {
        assert!(text.contains(label), "missing {label}:\n{text}");
    }
}
```

- [ ] **Step 8: Render the shared-axis tracks**

Implement `weathergram_render` with a bordered block and direct cell writes:

```rust
pub(super) fn weathergram_render(
    frame: &mut Frame,
    hours: &[HourlyForecast],
    palette: Palette,
    area: Rect,
    unit: Unit,
    selected: usize,
    compact: bool,
) {
    let window = window_for(area.width, selected, hours.len());
    let end = window.start.saturating_add(window.hours).min(hours.len());
    let visible = hours.get(window.start..end).unwrap_or_default();
    let block = Block::bordered().border_style(Style::new().fg(palette.border));
    let block = if compact {
        block
    } else {
        block.title(
            Line::from(format!("Hourly weather · next {} h", window.hours))
                .fg(palette.muted),
        )
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let plot_x = inner.x + LABEL_WIDTH;
    let plot_width = window.hours as u16 * window.cell_width;
    let summary_x = plot_x + plot_width + SUMMARY_GAP;
    let axis_y = if compact { area.y } else { inner.y };
    let first_track_y = if compact { inner.y } else { inner.y + 1 };
    let marker_y = if compact {
        area.bottom().saturating_sub(1)
    } else {
        inner.bottom().saturating_sub(1)
    };

    if compact {
        put_text(frame, area.x + 1, area.y, "Hourly", palette.muted);
    }
    hour_ticks_render(
        frame,
        Rect::new(plot_x, axis_y, plot_width, 1),
        visible.iter().map(|hour| hour.time.clone()),
        window.cell_width,
        palette,
    );

    let range = temperature_range(visible);
    let summaries = [
        String::new(),
        temperature_summary(visible, unit),
        precipitation_summary(hours, selected, unit),
        wind_summary(visible, unit),
    ];

    for (row, (label, summary)) in ["sky", "temp", "rain", "wind"]
        .into_iter()
        .zip(summaries)
        .enumerate()
    {
        let y = first_track_y + row as u16;
        put_text(frame, inner.x, y, label, palette.muted);
        put_right(frame, summary_x, y, SUMMARY_WIDTH, &summary, palette.muted);

        for (offset, hour) in visible.iter().enumerate() {
            let index = window.start + offset;
            let symbol = match row {
                0 => condition_symbol::symbol(hour.code),
                1 => temperature_step(hour.temp_c, range),
                2 => rain_step(hour.chance),
                _ => wind_symbol(hour.wind_kph, hour.wind_dir_deg),
            };
            let colour = if index == selected {
                palette.selection
            } else if index == 0 {
                palette.now
            } else if matches!(row, 1 | 2) {
                palette.accent
            } else {
                palette.text
            };
            let x = plot_x
                + offset as u16 * window.cell_width
                + (window.cell_width - 1) / 2;
            put(frame, x, y, symbol, colour);
        }
    }

    if window.start == 0 {
        put(frame, plot_x, axis_y, "┬", palette.now);
    }
    if selected >= window.start && selected < end {
        let offset = selected - window.start;
        let x = plot_x
            + offset as u16 * window.cell_width
            + (window.cell_width - 1) / 2;
        put(frame, x, marker_y, "▲", palette.selection);
    }
}
```

In full mode, add the top-border title `Hourly weather · next {hours} h`. In
compact mode, write only `Hourly` at the top-left so the clock ticks retain the
remaining border width; the pure horizon tests are the source of truth for the
12-hour compact choice.

For each hour, center its one-cell symbol within `cell_width`. Use
`palette.selection` for the selected column, `palette.now` for forward index
zero, and `palette.accent` for ordinary temperature/rain cells. Sky and wind
may use `palette.text`; labels, axes, and summaries use `palette.muted`.

In full mode, put clock ticks on the interior axis row and `▲` on the interior
marker row. In compact mode, write `Hourly` at the left of the top border,
clock ticks across the plot portion of that border, the current-hour mark on
the top border, and `▲` on the bottom border. Clip every write through the
existing `axis` helpers.

- [ ] **Step 9: Run renderer tests at every horizon**

Add a table-driven render assertion for widths 34, 80, 100, and 120 with titles
or axis text proving 12, 24, 36, and 48 hours. Also render an empty series and
an all-missing series without panic.

Run: `cargo fmt --check && cargo test --locked ui::weathergram::tests`

Expected: PASS.

- [ ] **Step 10: Commit the weathergram**

```bash
git add src/ui/mod.rs src/ui/weathergram.rs
git commit -m "feat: render adaptive hourly weathergram"
```

---

### Task 5: Replace the precipitation pane with the hourly inspector

**Files:**
- Move: `src/ui/precip.rs` → `src/ui/hourly.rs`
- Delete: `src/ui/precip_chart.rs`
- Modify: `src/ui/mod.rs:9-26,95-168`
- Modify: `src/ui/hourly.rs` after move
- Modify: `src/ui/axis.rs:1-7`
- Modify: `src/ui/bars.rs:1-29`
- Test: inline `src/ui/hourly.rs` and existing `src/ui/precip_week.rs` tests

**Interfaces:**
- Consumes: `weathergram_render`, `weathergram::FULL_ROWS`, `weathergram::COMPACT_ROWS`, and unchanged `precip_week_render`.
- Produces: `pub(super) fn hourly_render(frame: &mut Frame, app: &App, palette: Palette, area: Rect)`.
- Keeps: `next_precipitation`, snow-aware `amount_line`, positive-trace `measured`, selected-forward `window_from`, and `total_line` behavior from the old pane.
- Removes: peak-probability, wet-hour-count, mirrored-chart scale, and old chart rendering helpers.

- [ ] **Step 1: Move the module without changing behavior**

Run:

```bash
git mv src/ui/precip.rs src/ui/hourly.rs
```

In `src/ui/mod.rs`, change `mod precip;`/`use precip::precip_render;` to
`mod hourly;`/`use hourly::hourly_render;`, rename the function itself, and keep
it rendering the old chart for this setup step. Run `cargo check --locked` and
expect PASS before changing behavior.

- [ ] **Step 2: Write failing inspector and composition tests**

Replace old chart-specific expectations with:

```rust
#[test]
fn full_inspector_states_every_selected_hour_fact() {
    let app = app_showing(dry_hours(192), 3);
    let text = rendered(100, 24, &app);

    for label in ["feels like", "humidity", "precip", "wind", "24 h total"] {
        assert!(text.contains(label), "lost {label:?}:\n{text}");
    }
    assert!(text.contains("Hourly"), "weathergram missing:\n{text}");
}

#[test]
fn tall_layout_retains_the_weekly_precipitation_strip() {
    let app = app_showing(dry_hours(192), 0);
    let text = rendered(100, 30, &app);
    assert!(text.contains("Hourly"), "weathergram missing:\n{text}");
    assert!(text.contains("this week"), "weekly strip missing:\n{text}");
}

#[test]
fn compact_pair_uses_exactly_ten_content_rows() {
    let app = app_showing(dry_hours(192), 0);
    let text = rendered(34, 10, &app);
    assert_eq!(text.lines().count(), 10);
    for label in ["sky", "temp", "rain", "wind"] {
        assert!(text.contains(label), "lost {label}:\n{text}");
    }
}
```

- [ ] **Step 3: Run the focused tests and verify they fail on old content**

Run: `cargo test --locked ui::hourly::tests::full_inspector_states_every_selected_hour_fact`

Expected: FAIL because the old inspector has `amount`, `temperature`, `24 h peak`, and `wet hours`, and still calls the superseded chart path.

- [ ] **Step 4: Implement the responsive screen composition**

Use these content-derived row counts:

```rust
const FULL_INSPECTOR_ROWS: u16 = DIGIT_ROWS as u16 + 2;
const COMPACT_INSPECTOR_ROWS: u16 = 4;
const FULL_PAIR_ROWS: u16 = FULL_INSPECTOR_ROWS + weathergram::FULL_ROWS;
const COMPACT_PAIR_ROWS: u16 = COMPACT_INSPECTOR_ROWS + weathergram::COMPACT_ROWS;
```

Choose full layout when `area.height >= FULL_PAIR_ROWS`, otherwise compact. In
compact mode allocate exactly 4 inspector rows and 6 weathergram rows. In full
mode allocate 7 and 8, then compute weekly rows only from the remaining height:

```rust
let compact = area.height < FULL_PAIR_ROWS;
let inspector_rows = if compact { COMPACT_INSPECTOR_ROWS } else { FULL_INSPECTOR_ROWS };
let gram_rows = if compact { weathergram::COMPACT_ROWS } else { weathergram::FULL_ROWS };
let pair_rows = if compact { COMPACT_PAIR_ROWS } else { FULL_PAIR_ROWS };
let week_days = (!compact).then(|| week_days(hours, area, pair_rows)).flatten();

let [inspector, gram, week, _margin] = Layout::vertical([
    Constraint::Length(inspector_rows),
    Constraint::Length(gram_rows),
    Constraint::Length(week_days.map_or(0, precip_week::box_rows)),
    Constraint::Fill(1),
]).areas(area);
```

Render the weekly strip only when `week_days` is `Some`; preserve its existing
minimum width and minimum three-day rule.

- [ ] **Step 5: Replace the chance hero with selected temperature**

In full mode, render the selected `temp_c`, rounded after unit conversion, with
`big_digits`; append the two-character `Unit::temp_symbol()` to the center row.
If temperature is absent, render block-font `--` and the unit symbol. Keep the
existing city/condition top titles and next-precipitation/selected-time bottom
titles.

Generate the five full detail lines with these exact labels:

```rust
vec![
    detail_line("feels like", &feels_line(hour, unit), palette),
    detail_line("humidity", &humidity_line(hour), palette),
    detail_line("precip", &precip_line(hour, unit), palette),
    detail_line("wind", &wind_line(hour, unit), palette),
    detail_line("24 h total", &total_line(window_from(hours, selected), unit), palette),
]
```

`precip_line` combines probability with the existing snow-aware `amount_line`:

```rust
fn precip_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    let chance = hour.and_then(|h| h.chance).map(|value| format!("{value}%"));
    let amount = amount_line(hour, unit);
    match chance {
        Some(chance) if amount == UNKNOWN => chance,
        Some(chance) => format!("{chance} · {amount}"),
        None => amount,
    }
}
```

`wind_line` uses an eight-point compass label and formats `speed, gusts N
unit`; a missing direction must not suppress speed. `humidity_line` renders
`N%` or `UNKNOWN`.

Implement those helpers directly:

```rust
fn feels_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    hour.and_then(|h| h.feels_like_c).map_or_else(
        || UNKNOWN.to_string(),
        |c| format!("{:.0}{}", unit.temp(c), unit.temp_symbol()),
    )
}

fn humidity_line(hour: Option<&HourlyForecast>) -> String {
    hour.and_then(|h| h.humidity_pct)
        .map_or_else(|| UNKNOWN.to_string(), |value| format!("{value}%"))
}

fn compass(degrees: f64) -> &'static str {
    const POINTS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];
    POINTS[((degrees.rem_euclid(360.0) / 45.0).round() as usize) % POINTS.len()]
}

fn wind_line(hour: Option<&HourlyForecast>, unit: Unit) -> String {
    let Some(hour) = hour else { return UNKNOWN.to_string(); };
    let Some(speed) = hour.wind_kph else { return UNKNOWN.to_string(); };
    let direction = hour.wind_dir_deg.map(compass).map_or(String::new(), |d| format!(" {d}"));
    match hour.gust_kph {
        Some(gust) => format!(
            "{:.0}, gusts {:.0} {}{direction}",
            unit.speed(speed),
            unit.speed(gust),
            unit.speed_label(),
        ),
        None => format!("{:.0} {}{direction}", unit.speed(speed), unit.speed_label()),
    }
}
```

- [ ] **Step 6: Implement the two-line compact inspector**

Keep city/condition in the top border and selected time in the bottom border.
Use terse but complete interior lines that fit the 32-column inner minimum:

```text
77°F feels 79° RH58%
40% .01in SW9g15 24h.09in
```

Build them from the same formatting helpers as the full inspector. Use the
active speed and precipitation conversions before compacting labels. At wider
compact widths, allow separators (` · `); at 34 columns use single spaces.
Assert in a unit test that the metric and imperial worst-case strings are at
most 32 characters and never truncate mid-value.

Use a dedicated formatter rather than truncating the full strings:

```rust
fn compact_lines(
    hour: Option<&HourlyForecast>,
    ahead: &[HourlyForecast],
    unit: Unit,
) -> [String; 2] {
    let temp = compact_temp(hour.and_then(|h| h.temp_c), unit);
    let feels = compact_temp(hour.and_then(|h| h.feels_like_c), unit);
    let humidity = hour.and_then(|h| h.humidity_pct)
        .map_or_else(|| "—".to_string(), |value| format!("{value}%"));
    let chance = hour.and_then(|h| h.chance)
        .map_or_else(|| "—".to_string(), |value| format!("{value}%"));
    let amount = compact_amount(hour, unit);
    let wind = compact_wind(hour, unit);
    let total = compact_total(ahead, unit);

    [
        format!("{temp} feels {feels} RH{humidity}"),
        format!("{chance} {amount} {wind} 24h{total}"),
    ]
}
```

`compact_temp` emits `77°F`, `25°C`, or `—`; `compact_amount` emits `.01in`,
`0.2mm`, `1.0in` for snow, `0mm`/`0in` for a reported zero, or `—` for
missing; `compact_wind` emits `SW9g15`, `9g15` without direction, or `—`
without speed; and `compact_total` uses the same converted precision without a
space before the unit. Test each helper independently before the two-line width
assertion.

- [ ] **Step 7: Remove superseded chart code and tests**

Delete `src/ui/precip_chart.rs`, remove `mod precip_chart`, and remove imports
and tests concerned only with mirrored rising/falling columns, amount scale,
peak chance, wet-hour counts, or the old chart title. Keep tests for:

- next rain/snow wording;
- positive precipitation trace formatting;
- snow units;
- selected-forward 24-hour totals;
- missing values;
- border-title collision and truncation;
- weekly partial-day rows; and
- awkward-size no-panic rendering.

Update `axis.rs` and `bars.rs` module comments to describe the daily chart,
weathergram, and weekly strip rather than "both precipitation charts."

- [ ] **Step 8: Run hourly, weathergram, and weekly tests**

Run: `cargo fmt --check && cargo test --locked ui::hourly::tests && cargo test --locked ui::weathergram::tests && cargo test --locked ui::precip_week::tests`

Expected: PASS.

- [ ] **Step 9: Commit the integrated hourly screen**

```bash
git add src/ui/mod.rs src/ui/hourly.rs src/ui/weathergram.rs src/ui/axis.rs src/ui/bars.rs src/ui/precip_week.rs
git add -u src/ui
git commit -m "feat: replace precipitation chart with weathergram"
```

---

### Task 6: Lock down responsive and cross-screen regressions

**Files:**
- Modify: `src/ui/mod.rs:249-628`
- Modify: `src/ui/hourly.rs` test module
- Modify: `src/ui/weathergram.rs` test module
- Modify: `src/ui/legend.rs` test module
- Test: existing `src/app.rs` and `src/input.rs` test modules

**Interfaces:**
- Verifies, but does not change: `hourly_render` and `weathergram_render` public-to-UI interfaces from Tasks 4 and 5.
- Verifies: selected/current states have shape markers independent of palette.
- Verifies: both unit systems and all themes preserve identical cell positions.

- [ ] **Step 1: Add whole-app rendering tests at the required sizes**

In `src/ui/mod.rs`, render `Screen::Hourly` through the normal legend/content
split and add:

```rust
#[test]
fn hourly_screen_is_usable_at_the_declared_minimum() {
    let app = ready(Screen::Hourly);
    let buffer = drawn(&app, Theme::default().palette(), MIN_WIDTH, MIN_HEIGHT);
    let text = symbols(&buffer, MIN_WIDTH, MIN_HEIGHT).join("\n");

    for label in ["sky", "temp", "rain", "wind"] {
        assert!(text.contains(label), "minimum lost {label}:\n{text}");
    }
    assert!(!text.contains("Terminal too small"));
}

#[test]
fn hourly_height_tiers_drop_week_before_core_tracks() {
    let app = ready(Screen::Hourly);
    let short = symbols(&drawn(&app, Theme::default().palette(), 80, 12), 80, 12).join("\n");
    let tall = symbols(&drawn(&app, Theme::default().palette(), 100, 30), 100, 30).join("\n");

    assert!(!short.contains("this week"));
    assert!(tall.contains("this week"));
    for text in [&short, &tall] {
        for label in ["sky", "temp", "rain", "wind"] {
            assert!(text.contains(label), "lost {label}:\n{text}");
        }
    }
}
```

- [ ] **Step 2: Run the new integration tests and fix only integration defects**

Run: `cargo test --locked hourly_screen_is_usable_at_the_declared_minimum && cargo test --locked hourly_height_tiers_drop_week_before_core_tracks`

Expected: PASS after adjusting only row allocation, label widths, or test helper
return types. Do not remove a track or raise `MIN_WIDTH`/`MIN_HEIGHT` to satisfy the test.

- [ ] **Step 3: Extend theme and unit sweeps to the hourly screen**

Keep `Screen::Hourly` in the existing `states()` matrix and block-glyph color
test. Add an hourly test that renders metric and imperial at widths immediately
around each horizon boundary and asserts the symbol coordinates are identical;
only text values and unit suffixes may differ.

Also assert selected `▲` and the distinct current-hour axis mark exist in the
default palette and a monochrome probe palette, proving their state survives
without color differentiation.

- [ ] **Step 4: Exercise empty, missing, boundary, and stale-navigation states**

Add table-driven renders for:

- zero, one, and 192 forward hours;
- selected indices 0, a page edge, and the last valid hour;
- all optional new fields absent;
- temperatures below zero and a flat visible range;
- wind directions `-45`, `0`, `337.5`, and `360` degrees;
- rain chances at `9`, `10`, `29`, `30`, `49`, `50`, `69`, and `70`; and
- sizes 34×12, 80×24, 100×24, 120×30, and 200×50.

Keep the existing app navigation, search-return, refresh, and stale-response
tests; only rename their screen terminology.

- [ ] **Step 5: Run the locked all-target suite and Clippy**

Run: `cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked --all-targets`

Expected: PASS with the two provider-dependent live tests still ignored.

- [ ] **Step 6: Commit the regression coverage**

```bash
git add src/ui/mod.rs src/ui/hourly.rs src/ui/weathergram.rs src/ui/legend.rs src/app.rs src/input.rs
git commit -m "test: cover hourly weathergram layouts"
```

---

### Task 7: Update product documentation and complete acceptance

**Files:**
- Modify: `README.md:6-45,65-85,120-145`
- Modify: `Cargo.toml:4-10`
- Modify: comments found by the final terminology scan in `src/`
- Test: documentation commands and full project verification

**Interfaces:**
- Documents: `p` opens Hourly, four shared-axis tracks, adaptive 12/24/36/48-hour horizon, exact selected-hour inspector, and optional weekly precipitation strip.
- Removes: claims that the screen is a mirrored probability/amount precipitation chart.
- Does not require: uploading a replacement screenshot before code review; the spec makes the new capture a post-acceptance artifact.

- [ ] **Step 1: Replace the obsolete product copy**

Use this section in place of `## Hourly precipitation` and remove its outdated screenshot line:

```markdown
## Hourly weathergram

Press `p` for the hourly view. Sky, temperature, precipitation chance, and wind
share one clock, so a change in the forecast reads down a column instead of
across four separate tables. The selected-hour pane gives the exact feels-like
temperature, humidity, rain or snow amount, wind and gusts, and the following
24-hour precipitation total.

The visible horizon adapts from 12 hours on a narrow terminal to as many as 48
on a wide one. Arrow keys still browse the full eight-day hourly forecast, and
tall terminals retain the week-long precipitation strip below the weathergram.
```

Change the intro to `hourly conditions visualization`, the highlight to
`**Hourly weathergram**`, and the key table to say `hourly screen` and `Hourly
weathergram`. Replace the old center-rule explanation with a short explanation
of the shared axis, the `▲` selection marker, compact condition symbols, and
fixed rain-probability ramp.

Remove the brittle numeric test count from Engineering Quality: use `Virga's
locked deterministic test suite passes on Linux, macOS, and Windows` so adding
tests does not immediately stale the README again.

- [ ] **Step 2: Update package metadata and source terminology**

Set the Cargo description to:

```toml
description = "A responsive terminal weather app with an adaptive hourly weathergram, powered by Open-Meteo"
```

Run:

```bash
rg -n -i 'precipitation screen|precipitation-only|mirrored chance|precip_render|precip_chart|Screen::Precipitation|OpenPrecipitation' README.md Cargo.toml src
```

Expected: no stale product or typed-route terminology. Mentions of the weekly
precipitation strip, precipitation measurements, and historical rationale are
valid and should remain.

- [ ] **Step 3: Run documentation and package checks**

Run: `cargo fmt --check && cargo clippy --all-targets --locked -- -D warnings && cargo test --locked --all-targets && cargo package --locked`

Expected: all commands PASS; Cargo packages the spec, plan, source, and updated README without warnings.

- [ ] **Step 4: Commit the product documentation**

```bash
git add README.md Cargo.toml src
git commit -m "docs: explain hourly weathergram"
```

- [ ] **Step 5: Perform the real-terminal acceptance pass**

Run: `cargo run --release`

In both Ghostty and Apple's Terminal app:

1. Open the hourly screen with `p`.
2. Inspect a dry location and a mixed-rain location.
3. Resize across the 12-, 24-, 36-, and 48-hour width tiers.
4. Resize from 34×12 through a height that admits the weekly strip.
5. Hold every arrow key through page and day boundaries.
6. Toggle both unit systems and all five themes.
7. Confirm condition and wind symbols stay one cell wide, selection/current
   markers stay aligned, and no stale glyphs survive a resize.

Expected: the weathergram remains readable and unclipped. If the layout is not
accepted, stop on `feat/hourly-weathergram`; `main` remains untouched and the
visual design returns to brainstorming. If accepted, capture the new hourly
screen and replace the removed README image in a separate `docs:` commit after
the image has an accessible hosted URL.

---

## Final Self-Review Checklist

- Every field in the design spec is requested, parsed, modeled, rendered, and tested.
- Missing `code` renders blank; an unknown reported numeric code renders `?`.
- 12/24/36/48-hour thresholds come from measured plot width and remain stable within a page.
- The compact 4-row inspector plus 6-row weathergram fits the 10 content rows left at 34×12.
- The weekly strip is retained only after the comfortable pair receives its rows.
- `p` remains the only advertised shortcut; no hidden `h` alias or new view mode exists.
- The old mirrored chart and its stale documentation are removed only after replacement coverage passes.
- All required locked verification commands and both manual terminals have been exercised.
