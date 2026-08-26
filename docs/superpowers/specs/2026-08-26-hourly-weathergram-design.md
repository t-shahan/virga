# Hourly Weathergram Design

## Summary

Virga will evolve its precipitation screen into a full hourly weathergram. The
screen will preserve the existing eight-day hourly navigation and long-range
precipitation strip while adding sky, temperature, precipitation-probability,
and wind tracks on one shared time axis.

The result should answer two questions without requiring a mode change:

1. What will the next several hours feel like?
2. When will conditions change enough to affect a plan?

This is an experimental feature on `feat/hourly-weathergram`. The branch must
remain independently testable and may be abandoned without changing `main` if
the real-terminal result is too dense.

## Goals

- Provide the full hourly conditions view expected in a weather application.
- Make the view distinctly Virga: terminal-native, keyboard-driven, compact,
  and organized around readable patterns rather than a grid of repeated text.
- Preserve precipitation as an important part of the screen without allowing
  it to dominate every hourly decision.
- Reuse Virga's existing request, state, unit, theme, and navigation boundaries.
- Remain useful from the 34×12 minimum terminal through wide and tall layouts.
- Preserve meaning without relying on color alone.

## Non-goals

- A separate top-level hourly screen in addition to the precipitation screen.
- Interactive per-variable chart modes or switchable lenses.
- A derived "wetness", comfort, or other combined weather score.
- Weather alerts, radar, notifications, caching, or saved-location changes.
- A new data provider or an additional network request.
- Changes to the daily weather screen beyond renaming its hourly shortcut.

## User Experience

### Entry and navigation

The current precipitation screen becomes the hourly screen. `p` remains its
stable shortcut, while the weather-screen key legend changes its label from
`precip` to `hourly`. Existing navigation is unchanged:

- `Left` and `Right` move one hour and wrap across the forward forecast.
- `Up` and `Down` move one day while retaining the selected time of day.
- `n` returns to the current hour.
- `b`, `Enter`, `Esc`, or `p` returns to the daily weather screen.
- Search, refresh, units, themes, and quitting retain their current behavior.

The application enum and rendering terminology should use `Hourly` rather than
`Precipitation`, but `p` is deliberately retained for muscle memory and
backward compatibility.

### Screen hierarchy

The hourly screen has three layers, in priority order:

1. A selected-hour inspector.
2. A shared-axis weathergram.
3. The existing week-long precipitation strip, only when surplus height remains.

The inspector and weathergram are always present at supported terminal sizes.
The weekly strip must disappear before it can compress either of them below
their compact layouts.

### Selected-hour inspector

The top bordered pane identifies the location, selected local time, and weather
condition. At comfortable heights it uses large temperature digits as its hero.
Its detail column contains five readings:

- feels-like temperature;
- relative humidity;
- precipitation probability and amount for the selected hour;
- wind direction and speed, including gust speed when present; and
- total precipitation over the 24 hours beginning at the selected hour.

The existing next-precipitation message remains on the lower-left border. It is
computed from the current hour rather than the selection, because it describes
the world now. The selected local date and time remain on the lower-right.

At short heights, the large temperature digits disappear and the same facts are
rendered in a compact text arrangement. Missing values display an em dash. No
detail is repeated merely to fill available space.

### Weathergram

The weathergram aligns four tracks to the same hour columns:

1. `sky`: one-cell condition symbols;
2. `temp`: a five-level temperature silhouette;
3. `rain`: a five-level precipitation-probability silhouette; and
4. `wind`: one-cell direction symbols.

One clock axis serves every track. The selected hour is marked with `▲` beneath
its column. The current hour is marked distinctly on the axis. Theme colors
reinforce these states, but the shape markers carry their meaning without color.

Track labels remain visible at every supported width. Summaries at the right
show the visible temperature range, precipitation total for the 24 hours
beginning at the selection, and visible wind-speed range. Exact selected-hour
values live in the inspector rather than being repeated on the tracks.

### Adaptive time horizon

The time horizon is quantized rather than continuously resized. After
subtracting borders, track labels, gaps, and right summaries, the renderer
chooses the horizon from the remaining plot width:

- Choose 48 hours when the plot can give all 48 hours at least two cells each.
- Otherwise choose 36 hours under the same two-cells-per-hour rule.
- Otherwise choose 24 hours under the same two-cells-per-hour rule; this is the
  normal layout.
- Otherwise choose 12 hours with at least one cell per hour; this is the compact
  layout and is guaranteed to fit at Virga's supported minimum width.

The exact thresholds derive from measured content widths rather than magic
terminal sizes. Resizing within a tier cannot change the horizon. The existing
selection-window behavior keeps the selected hour visible; moving beyond its
edge advances the window, and all eight forecast days remain reachable.

### Weekly precipitation strip

`ui/precip_week.rs` remains the final, optional layer. It continues to answer
"which day looks wet?" across the full forecast and retains its present
probability shading, totals, selected-day marker, and width requirements.

It appears only after the inspector and weathergram receive their comfortable
heights and at least three weekly rows fit. It is the first layer removed as
height decreases.

## Visual Semantics

### Condition symbols

Condition symbols must occupy exactly one terminal cell according to the width
logic used by Ratatui. They group WMO weather codes as follows:

| Condition | Codes | Symbol |
|---|---:|:---:|
| Clear | 0 | `○` |
| Mainly clear / partly cloudy | 1, 2 | `◐` |
| Overcast | 3 | `●` |
| Fog | 45, 48 | `≡` |
| Drizzle / freezing drizzle | 51, 53, 55, 56, 57 | `┆` |
| Rain / freezing rain / showers | 61, 63, 65, 66, 67, 80, 81, 82 | `│` |
| Snow / snow grains / snow showers | 71, 73, 75, 77, 85, 86 | `*` |
| Thunderstorm | 95, 96, 99 | `ϟ` |
| Missing or unknown | any other value | `?` |

The selected-hour inspector always spells out the condition, so the compact
symbols support pattern recognition without becoming the only explanation.

### Temperature

The temperature track uses `▁`, `▂`, `▄`, `▆`, and `█`, scaled between the
minimum and maximum non-missing temperatures in the visible window. When every
available temperature is equal, each point uses the middle `▄` step. Missing
hours render a blank cell and do not affect the scale.

The visible minimum and maximum appear in the right summary using the active
unit. The scale may change when the visible window changes, but not when only
the selection moves within that window.

### Precipitation

The precipitation track shows probability only. It reuses Virga's established
five-step ramp:

- below 10%: `·`;
- 10–29%: `▂`;
- 30–49%: `▄`;
- 50–69%: `▆`;
- 70–100%: `█`.

Missing probability is a blank, not a dry claim. Hourly precipitation amount
and the selected-forward 24-hour total remain exact text in the inspector. The
right summary repeats only the total so a heavy event is not mistaken for a
high-probability drizzle pattern.

### Wind

Wind direction uses eight one-cell arrows: `↑`, `↗`, `→`, `↘`, `↓`, `↙`, `←`,
and `↖`. Calm wind is `·`; a missing direction is blank. Calm means a reported
speed below 1 km/h before unit conversion. The right summary shows the minimum
and maximum reported speed in the visible window using the active unit.

## Architecture

### Domain and provider boundary

`HourlyForecast` gains independently optional fields:

- `feels_like_c: Option<f64>`;
- `humidity_pct: Option<u8>`;
- `wind_kph: Option<f64>`;
- `gust_kph: Option<f64>`; and
- `wind_dir_deg: Option<f64>`.

`HourlyDto` gains corresponding defaulted parallel arrays for Open-Meteo's
`apparent_temperature`, `relative_humidity_2m`, `wind_speed_10m`,
`wind_gusts_10m`, and `wind_direction_10m` hourly fields. Conversion indexes
each array independently. The timestamp remains the only required hourly value;
a missing or shorter measurement array yields `None` for that field without
dropping or shifting the hour.

`fetch_daily_with` adds these variables to its existing `hourly` query. Virga
continues to make one forecast request and one concurrent air-quality request
per weather load. There is no new loading state or provider dependency.

### Application state

`Screen::Precipitation` becomes `Screen::Hourly`. The selected hour remains the
only weathergram-specific application state. Window start, horizon, track
scales, and responsive tiers are pure rendering calculations derived from the
weather data, selection, unit, and available rectangle.

No chart settings or transient view modes are persisted.

### Rendering modules

- `src/ui/hourly.rs` composes the hourly screen and renders the responsive
  selected-hour inspector.
- `src/ui/weathergram.rs` owns horizon selection, the shared axis, track
  orchestration, selection/current markers, and right-side summaries.
- `src/ui/condition_symbol.rs` maps WMO codes to verified one-cell symbols.
- `src/ui/precip_week.rs` remains the optional long-range layer.
- `src/ui/precip.rs` and `src/ui/precip_chart.rs` are removed after their
  required behavior and test coverage have moved to the new modules.

The UI remains free of networking. Track calculations should be small pure
functions so scaling, missing values, width accounting, and glyph selection can
be tested independently of complete screen renders.

## Responsive Behavior

Layout allocation follows this order:

1. Reserve the key legend measured for the current width.
2. Choose the full or compact inspector and weathergram pair from available
   height.
3. Give the weathergram its four tracks, axis, marker, and borders.
4. Add the weekly strip only if its existing minimum width and at least three
   day rows fit.
5. Leave any remaining rows as margin rather than blank space inside a bordered
   chart.

The full pair uses a seven-row inspector and an eight-row weathergram: four
tracks, a dedicated clock-axis row, a dedicated selection-marker row, and two
borders. The compact pair uses a four-row inspector and a six-row weathergram.
The compact inspector keeps location and condition in its top border, selected
time in its bottom border, and two interior lines for the selected readings.
The compact weathergram moves clock labels into its top border and current and
selection markers into its bottom border, leaving one interior row per track.

Together with the existing two-row compact key legend, the compact pair fits
exactly at Virga's supported 34×12 minimum. Below that, the existing size
warning continues to replace the UI.

Width reduction changes the horizon before removing track content. No supported
width may clip a label, summary, glyph, or key binding mid-cell.

## Failure and Missing-data Behavior

- Every new provider measurement is optional from DTO through rendering.
- A missing field blanks only its cell or inspector value.
- Missing probability is distinct from zero probability.
- Missing wind direction does not suppress a reported wind speed in the
  inspector or visible speed range.
- An entirely absent hourly block preserves the existing empty-hourly behavior
  and must not panic.
- Forecast failure, refresh retry, stale-response rejection, and search return
  behavior remain unchanged.
- A provider response that lacks all newly requested arrays still renders the
  temperature and precipitation information Virga already receives.

## Testing

### DTO and client tests

- Parse all five new hourly fields into the matching hour.
- Treat nulls, absent arrays, and shorter parallel arrays as field-level
  absence without dropping hours.
- Prove the forecast request includes the new hourly variable names.
- Preserve the existing two-request weather-load behavior and partial AQI
  failure behavior.

### Application and input tests

- Rename screen expectations to `Hourly` without changing navigation
  invariants.
- Keep `p` opening and closing the hourly screen from the daily weather screen.
- Preserve hourly wrapping, day jumps, return-to-now, search-return, refresh,
  theme, and unit behavior.

### Rendering and calculation tests

- Verify every condition symbol is exactly one terminal cell wide and every
  documented WMO code maps to the intended category.
- Verify temperature scaling for ranges, flat values, negative values, and
  missing readings.
- Verify precipitation thresholds and missing-versus-zero behavior.
- Verify eight wind sectors, calm handling, missing direction, and unit-aware
  speed summaries.
- Verify horizon selection and window stability at each width tier.
- Verify selection and current markers are present without inspecting color.
- Render deterministic snapshots with Ratatui's `TestBackend` at 34×12, 80×24,
  a wide terminal, and a tall terminal that admits the weekly strip.
- Exercise every theme and both unit systems at boundary widths.

### Manual verification

- Run the release build in at least Ghostty and Apple's Terminal app.
- Inspect one dry forecast and one mixed rain forecast.
- Resize across every horizon and height tier.
- Check font fallback, arrow and condition-symbol alignment, fast held-key
  navigation, and the absence of stale cells after resize.

## Compatibility and Constraints

- Rust 1.88 remains the minimum supported version.
- No new dependency is required.
- Open-Meteo remains the only weather-data provider.
- A weather load remains two network requests.
- English-only copy and the five foreground-only themes remain unchanged.
- Existing persisted location data requires no migration.
- The locked deterministic suite, rustfmt, Clippy with warnings denied, and
  packaging checks must pass before the branch is offered for review.

## Documentation

The README will be updated on the feature branch to:

- replace the dedicated hourly-precipitation description with the hourly
  weathergram;
- explain the four tracks, adaptive horizon, and exact selected-hour inspector;
- retain the precipitation-probability versus amount distinction;
- update the `p` key label to `hourly`; and
- replace the hourly screenshot or animation after the real-terminal design is
  accepted.

The screenshot update is the final acceptance artifact, not a prerequisite for
implementing or testing the feature.
