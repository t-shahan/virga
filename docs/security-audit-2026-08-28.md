# Security audit — 2026-08-28

A full audit of the Virga codebase at commit `8c535b2`: every Rust source
file, `install.sh`, both maintenance scripts, all three GitHub Actions
workflows, the dependency tree, and the git history.

## Scope and method

- **Manual review** of the attack-surface code: `src/weather/client.rs`
  (network), `src/update.rs` (release probe), `src/state.rs` (persistence),
  `src/main.rs` / `src/cli.rs` (entry points), `src/app.rs` (state machine),
  `install.sh`, `scripts/*.sh`, `.github/workflows/*.yml`.
- **Adversarial review of the parsing and rendering layer**
  (`src/weather/dto.rs`, `model.rs`, `code.rs`, all of `src/ui/`,
  `input.rs`, `events.rs`, `units.rs`, `theme.rs`) treating every byte from
  the Open-Meteo and ipapi.co APIs as attacker-controlled.
- **Dependency audit**: `cargo audit` (RustSec advisory database,
  1226 advisories) against all 266 crates in `Cargo.lock`.
- **History scan** of all 95 commits for committed secrets.

## Threat model

Virga runs unprivileged, listens on nothing, and executes nothing. Its
inputs are: HTTPS responses from three Open-Meteo hosts and ipapi.co, an
HTTPS redirect from github.com, the local state file, environment
variables, and keystrokes. The interesting attackers are a compromised or
impersonated API endpoint (or a TLS-breaking middlebox), a hostile local
state file, and the release/supply chain.

## Summary

No high- or critical-severity issues were found. `cargo audit` reports zero
advisories. There are no `unsafe` blocks, no subprocess execution, no
secrets in history, and the CI/release pipeline is hardened well beyond
what projects of this size usually bother with. The findings below are
low-severity hardening opportunities, listed most interesting first.

## Findings

### V-1 (Low): API-sourced strings reach the raw terminal unsanitized

Inside the TUI, ratatui strips control characters at render time
(`ratatui-core` filters `char::is_control` in `Buffer::set_stringn` and
`Span`), so hostile strings cannot inject escape sequences there. But
three paths print network-derived strings to the plain terminal with
`println!`/`eprintln!`, where no such filter exists:

- `src/main.rs:155` — the startup update notice, and `src/main.rs:78` —
  the `virga update` report. Both embed the latest `Release`, whose
  pre-release suffix (`update.rs:82-88`) accepts **any** characters after
  the `-`. The string originates from GitHub's `Location` header
  (`update.rs:54`). A redirect to `.../tag/v99.0.0-<ESC>]0;...<BEL>`
  would print terminal escape sequences (title changes, screen
  manipulation, and on some emulators worse) on every launch.
- `src/main.rs:150` (via `app.rs:376-379`) — the detection-failure
  warning embeds the persisted location label verbatim. Labels originate
  from ipapi.co / Open-Meteo geocoding responses, are persisted to
  `state.json` without character-level sanitization (`state.rs:96` checks
  only for non-empty), and are later printed raw to stderr.

Exploitation requires a compromised provider, a CA-level TLS break, or a
hand-edited state file — hence Low. But the fix is cheap and closes the
class:

- In `Release::parse`, restrict the pre-release suffix to
  `[0-9A-Za-z.-]` (this is also what semver allows; anything else is
  already "not a version").
- Strip `char::is_control` from labels at ingestion (in
  `GeoIpDto::into_location` / `GeocodeResultDto::into_location`), or at
  the `eprintln!` boundary.

### V-2 (Low): install.sh checksum verifies integrity, not authenticity

`install.sh` downloads `SHA256SUMS` from the same GitHub release it
downloads the archive from (lines 100-103). This catches truncation and
mirror corruption, but an attacker who can alter the archive can alter the
checksum file identically. The project already produces the stronger
artifact — build provenance attestations (`release.yml:136-138`) — and the
release notes even document `gh attestation verify`. The script could
optionally run that verification when `gh` is present, and say so when it
is not. Also worth noting: the curl-pipe-to-sh pattern itself is an
accepted-risk choice the README makes deliberately; the script's internal
hygiene (staged atomic install, `set -eu`, no sudo) is exemplary.

### V-3 (Low): geoIP lookup sends the user's IP to a third party by default

`https://ipapi.co/json/` is contacted on first launch (and on launches
where no chosen location is remembered) unless `VIRGA_GEOIP=off`. This is
disclosed in `--help` and the README, uses HTTPS, and sends nothing but
the connection itself — but it is still an on-by-default disclosure of the
user's IP-derived location to a party other than Open-Meteo. A privacy
paragraph in the README naming ipapi.co explicitly (the README currently
credits Open-Meteo only) would make the disclosure complete.

### V-4 (Info): update probe runs on every launch by default

`spawn_update_check` (`main.rs:415-422`) contacts github.com on every
launch unless `VIRGA_UPDATE=off`. It is redirect-only, 5-second bounded,
and sends no identifying data beyond the connection, but combined with
V-1 it is the one path where a github.com response influences terminal
output outside ratatui. With V-1's parse tightening this becomes fully
inert.

### V-5 (Info): tar extraction trusts the archive layout

`install.sh:117-118` extracts the release archive and then locates the
binary with `find`. Modern GNU tar and bsdtar refuse `..` and absolute
member paths by default, so path traversal is not reachable on supported
platforms, and the extraction happens inside a fresh `mktemp -d`
directory in any case. No action needed; recorded so the reasoning is on
file.

## What was checked and found clean

- **Memory safety**: no `unsafe` anywhere in the crate; overflow-prone
  arithmetic uses `saturating_*`, `checked_*`, `rem_euclid`, and guarded
  indexing (`.get()`); `hour.time.get(11..13)` handles non-UTF-8
  boundaries safely. Selection indices are reset on every `Loaded`
  message, so stale indices cannot outlive the data they index.
- **Hostile API data**: the DTO layer treats every measurement as
  optional; required top-level blocks are enforced so an empty body is an
  error, not an empty screen. Response bodies are capped at ureq's 10 MB
  default before JSON parsing, bounding allocation. Non-finite and
  out-of-range coordinates are rejected at every boundary (client,
  state-file load, state-file save). Rendering code was exercised against
  degenerate sizes and data in the existing test suite and reviewed for
  panicking indexing; the UI layer's series lengths are derived from the
  data actually present, not from assumed shapes.
- **Network**: TLS via rustls with webpki-roots (no system-store or
  plaintext fallback); every agent carries explicit connect and global
  timeouts, verified by tests; query parameters are built with ureq's
  encoder, so the search box cannot inject URL structure; the geocoding
  `count=5` caps result volume; air-quality failure degrades without
  taking the forecast down.
- **State file**: versioned, validated on load, written via
  tempfile-and-rename with fsync, serialized across processes with an OS
  file lock, refuses to overwrite documents from newer versions or files
  it cannot read. Malformed documents degrade to defaults with a warning.
- **Command line & environment**: no arguments reach a shell; unknown
  arguments and env values are echoed with `{:?}` (which escapes control
  characters); `VIRGA_THEME`/`VIRGA_GEOIP`/`VIRGA_UPDATE` typos warn and
  fall back rather than fail. The `virga update` instruction shell-quotes
  the install directory against metacharacters, with tests proving it.
- **Self-update**: `virga update` never downloads or writes anything — it
  prints an instruction matched to the install method. There is no
  auto-update code path to compromise.
- **Supply chain / CI**: every action pinned to a commit SHA with the
  version in a comment; workflow permissions default to `contents: read`
  and are raised per-job only where needed; untrusted inputs
  (`workflow_dispatch` tag, commit subjects) reach shell only via
  environment variables, never interpolated; actionlint is fetched by
  pinned checksum; `cargo install cargo-audit` is version-pinned and
  `--locked`; the advisory audit runs weekly on a schedule, not only on
  pushes; release binaries carry build provenance attestations; the
  Homebrew tap push uses a single-repo deploy key with GitHub's SSH host
  key pinned in `known_hosts` and `IdentitiesOnly=yes`; the tap formula
  reads checksums back off the published release rather than trusting
  values carried between jobs, and double-checks for empty digests.
- **Secrets**: no credentials, tokens, or key material in any of the 95
  commits.
- **Dependencies**: `cargo audit --deny warnings` passes with zero
  advisories against RustSec (1226 advisories loaded, 266 crates
  scanned).

## Suggested follow-ups, in order

1. Tighten `Release::parse` to reject non-`[0-9A-Za-z.-]` pre-release
   suffixes, and strip control characters from location labels at
   ingestion (V-1).
2. Mention ipapi.co by name in the README's data-source/privacy prose
   (V-3).
3. Consider opportunistic `gh attestation verify` in `install.sh` (V-2).
