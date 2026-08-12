# Remember the Last Successful Location

## Goal

Virga should start with the last location whose weather loaded successfully.
On a first run, or whenever saved state cannot be used, it should start in New
York City instead of Frederick, Maryland.

This feature remembers a location only. It does not cache weather, search
queries, units, themes, or browsing history, so every launch still fetches
fresh data from Open-Meteo.

## User-visible behavior

- The built-in fallback is `New York City, New York, United States` at
  `40.7128, -74.0060`.
- With no saved state, the first request uses that fallback.
- After a searched-for location loads successfully, subsequent launches start
  there.
- A failed location fetch never replaces the saved location.
- Missing, malformed, unsupported, or invalid state falls back to New York City
  instead of preventing startup.
- State read and write failures are non-fatal. A warning is shown outside the
  alternate terminal screen when it can be shown without damaging the TUI.

The README will explicitly disclose that Virga stores the last successful
location's display label and coordinates locally. It will continue to state
that weather and search history are not cached.

## Storage format and location

A new `state` module owns persistence. It stores a small, versioned JSON
document containing only:

```json
{
  "version": 1,
  "location": {
    "label": "New York City, New York, United States",
    "lat": 40.7128,
    "lon": -74.006
  }
}
```

The file lives in the operating system's conventional per-user state or data
directory under a `virga` directory. Path selection is isolated behind the
state module so callers do not depend on platform details.

Writes replace the state atomically through a temporary file in the same
directory. A failed replacement must leave the previous valid state usable.

Loaded coordinates are accepted only when both are finite, latitude is within
`-90..=90`, longitude is within `-180..=180`, and the trimmed label is not
empty. The version field makes later schema changes explicit; unknown versions
are ignored rather than guessed at.

## Architecture and data flow

`ActiveLocation` remains the application's location value and gains the serde
traits needed by the state document. Its `Default` implementation becomes New
York City.

At startup, before Ratatui takes over the terminal, `main` asks the state module
for the remembered location. A valid saved location is supplied to `App`; any
absence or failure supplies `ActiveLocation::default()`.

`App` remains free of filesystem access. It already changes `app.location`
only while accepting a matching successful `Message::Loaded`. The event loop
uses that transition as the persistence boundary: stale responses and failed
fetches cannot be saved because neither changes the active location.

Persistence errors never change `App` state and never stop the draw loop. The
event loop retains at most one warning for display after the terminal has been
restored. Startup read warnings can be printed before terminal initialization.

## Dependencies

Prefer a small established crate for platform-appropriate user directories
rather than maintaining incomplete operating-system path rules. Promote
`serde_json` from a development-only dependency to a runtime dependency for
the versioned state document. Any added dependency must support the project's
Rust 1.88 minimum.

## Testing

Tests exercise the state module through injected temporary paths rather than
the real user directory:

- a saved location round-trips exactly;
- a missing file means no remembered location;
- malformed JSON, unknown versions, empty labels, non-finite coordinates, and
  out-of-range coordinates are rejected;
- a failed write does not destroy an existing valid state file;
- `ActiveLocation::default()` is New York City;
- only an accepted successful weather response is eligible for persistence;
  failed and stale responses leave the previous location unchanged.

The full existing test suite, formatting check, Clippy with warnings denied,
and locked all-target test command must remain green.

## Out of scope

- A general configuration file or settings screen.
- Weather-response caching or offline mode.
- Location favorites or multiple saved locations.
- Persisting units or the theme from the open theme pull request.
- Migration from the old Frederick fallback, because it was compiled in and
  never written to disk.
