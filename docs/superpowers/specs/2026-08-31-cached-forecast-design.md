# Cached forecast: paint the last forecast first

## Goal

Make launch feel instant for a returning user. Every launch today opens on a
spinner and waits for Open-Meteo, 0.5 s on a warm connection and 1 to 2 s on
a cold one (#54). The app forgets the forecast the moment it exits and
rebuilds it from nothing next time.

Keep the last successful forecast on disk. On launch, if it describes the
remembered city and is under 24 hours old, paint it as the first frame with
its age marked, dispatch the fetch as before, and replace the numbers in
place when the fresh forecast lands. The same mechanism makes `r` refresh
without blanking the screen, and turns a failed refresh from an error popup
into a marked, still-usable forecast.

## User-visible behavior

### Launch with a cache

The first frame is the cached forecast, drawn at the moment the alternate
screen opens. The current pane's bottom border carries a muted mark on the
left, where the period comparison sits today:

```
as of 17:52 · ⠋ updating
```

The spinner glyph animates while the fetch is in flight. When the fresh
forecast arrives the numbers change in place, the mark goes, and the
comparison sentence returns. No clear, no flash.

A cache from an earlier calendar day says so: `as of yesterday 22:14`. The
time is the fetch time on the user's clock, not the city's.

Where the pane is too narrow for both the mark and the day, the day wins,
the rule `bottom_titles` already applies to the comparison.

### Launch without a cache

Unchanged. A first run, a city other than the cached one, a cache over 24
hours old, a cache this binary cannot read, or `VIRGA_CACHE=off` all open on
the spinner as today.

### Refresh

`r` keeps the weather on screen and shows `⠋ updating` in the mark instead
of replacing the screen with the popup. Choosing a new city still shows the
popup, because the weather on screen describes somewhere else.

The rule underneath: **the weather on screen stays while the pending fetch
targets the same place it describes.** Everything above follows from it.

### A fetch that fails

With weather on screen, the weather stays and the mark turns the error
colour:

```
as of 17:52 · refresh failed, r to retry
```

or `refresh failed, r to retry` when the forecast on screen was fetched live
this session. The full error text is printed after exit, with the other
recoverable complaints. With nothing on screen the error popup appears as
today.

### The hourly screen

The same mark appears in the hourly screen's bottom border, so a cached or
failed forecast is never shown without its label whichever screen is up.

### `VIRGA_CACHE`

`off` skips both the read and the write. Parsed by the same `switch` as
`VIRGA_GEOIP` and `VIRGA_UPDATE`: a value that is neither is a warning and
the default, on. The help text gains the line.

## Storage

A second file beside `state.json`, `forecast.json`, in the same directory.
`state.json` is not touched by this change and its format does not learn
about the cache.

```json
{
  "version": 1,
  "location": {"label": "Frederick, Maryland, United States", "lat": 39.41, "lon": -77.41},
  "fetched_at": 1788471120,
  "weather": { ... the parsed Weather ... }
}
```

- `fetched_at` is UTC seconds. The age bound and the "yesterday" wording are
  computed from it against the clock at load.
- `weather` is the parsed model, so `Weather` and the structs it holds derive
  `Serialize` and `Deserialize`. `Current` gains `observed`, the response's
  `current.time`, which the relocation below needs.
- The write is the same tempfile, fsync, rename as `state.json`, under its own
  lock file, `forecast.json.lock`.
- The version is the cache's own. A model change bumps it; an old cache is
  discarded, never migrated. A version above what this binary knows reads as
  no cache and is not overwritten, the rule `state.json` follows.
- Roughly 50 KB. Read and parsed before the terminal is taken over.

### Reading

`load` returns no cache, silently, when the file is missing, describes other
coordinates than the remembered city, or is older than 24 hours. A file that
exists but cannot be read or parsed is a warning printed after exit and no
cache; a launch is never refused over it.

### Relocating "now"

`Weather::now_hour` and `today_index` are positions the parser computes from
`current.time`. A cached forecast's "now" is the hour it was fetched. At load,
local time at the city is `observed` plus the wall clock elapsed since
`fetched_at`, and both positions are recomputed from that stamp by the same
helper the parser uses, moved out of `dto.rs` onto `Weather`. If the target
hour is not in the series the cache is refused. The hourly series carries a
day of history and eight days forward, so anything under the 24 hour bound
relocates.

### Writing

The worker thread writes the cache after a successful fetch, after it has
sent `Loaded` to the app, so the fsync never sits between the response and
the frame. A write that fails sends `Message::CacheFailed`, which becomes an
exit warning. The worker takes the cache path at spawn; `None` disables the
write.

## Architecture

- `src/cache.rs`, new: the document, `load`, `save`, the age bound, the
  "as of" wording. Pure over a path and a clock passed in.
- `src/weather/model.rs`: serde derives, `Current::observed`,
  `Weather::relocate(stamp)` holding the position logic from `dto.rs`.
- `src/app.rs`: `Startup::cached`; `App::shown` recording the age label of
  the weather on screen and whether its last refresh failed;
  `App::is_refreshing`; `fetch` applies the same-place rule; `refresh` guards
  on a pending request rather than on `Fetch::Loading`; `LoadFailed` keeps
  weather that is showing.
- `src/events.rs`: `CacheFailed`; the worker writes the cache.
- `src/main.rs`: `VIRGA_CACHE`, loading the cache beside the state file,
  the draw loop animating while refreshing.
- `src/ui/current.rs` and `src/ui/hourly.rs`: the mark.
- `src/cli.rs`: the help line.

## Tests

- `cache`: round trip through a file; the age bound at either side of 24
  hours; coordinate mismatch; a version above this binary's; a corrupt body
  is an error, not a panic; "as of" wording for today and yesterday.
- `model`: `relocate` across an hour boundary and a day boundary, and
  refusal past the series.
- `app`: a startup with a cache opens `Ready`; a refresh of the shown place
  keeps it; a fetch for another place does not; `LoadFailed` keeps shown
  weather and marks it; `Loaded` clears the mark.
- `ui`, through `TestBackend`: the mark while updating, after a failure, at
  34 columns, and its absence on live weather; the hourly screen's copy.
- `main`: `VIRGA_CACHE` parsing; `composition` unchanged by a cached start.

No test touches the network or the clock. The loader takes the time as an
argument.

## Out of scope

- `virga now` neither reads nor writes the cache.
- No cache for searches or detection.
- No skeleton layout for a cache miss.

## Documentation

- README: rewrite the Limitations paragraph that promises weather is never
  cached; a paragraph in Data and Privacy on what the file holds and that it
  never leaves the machine; the second file in the uninstall table;
  `VIRGA_CACHE` in the environment table.
- Changelog under Unreleased.
