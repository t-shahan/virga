# Virga

A terminal weather app in Rust. It reads Open-Meteo over the network, renders
with Ratatui, and persists a small amount of state to the user's config
directory. `src/ui/` draws, `src/weather/` fetches and parses, and the files
at the top of `src/` hold the app loop, input, state, and CLI.

## Checks

The four gates CI enforces, and the same four `scripts/release.sh` runs before
it will tag:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

The minimum supported Rust version is 1.89, which is a real constraint rather
than a rounded-down guess. `File::lock` stabilized there and serializes
concurrent state saves. CI builds against exactly 1.89 to catch a dependency
raising the floor.

Tests that reach a live provider stay `#[ignore]`. An Open-Meteo outage is not
a reason to fail somebody's pull request.

## Comments

Comments here explain why a thing is the way it is, not what the next line
does. The ones worth writing record a constraint, a rejected alternative, or a
trap. See the header of `.github/workflows/ci.yml` or the `rust-version` note
in `Cargo.toml` for the register.

A comment that restates the code is worse than no comment, because it rots
independently.

## Commits and changelog

Conventional commits: `feat`, `fix`, `perf`, `docs`, `test`, `ci`, `chore`,
`refactor`. Imperative mood.

Work a user can observe goes in `CHANGELOG.md` under the topmost section.
`scripts/check-changelog.sh` gates this and will refuse a `feat`, `fix`, or
`perf` that describes nothing, and refuse any edit to a section that has
already shipped. Both are escapable by a trailer, `Changelog: none` and
`Changelog: history`, when the exception is real.

## What a code review must check

These are requirements, not preferences. A review of a change to this
repository is expected to report a violation of any of them, including when
the violation is a general quality problem rather than a specific bug.

### Security

Virga parses JSON it did not write, from Open-Meteo and from GitHub's release
endpoint, and it writes files into the user's config directory. Review must
check:

- Every outbound HTTP call sets a timeout. A hung socket freezes the interface.
- Responses are size-bounded before being read into memory.
- Absent, null, and wrong-typed JSON fields deserialize without panicking.
- Paths derived from user or network input cannot escape the intended
  directory, and state writes stay atomic.
- No untrusted value is interpolated into a shell command or into a workflow
  `run:` block. In workflows, pass it through `env:` and quote the variable.
- `install.sh` is piped straight into strangers' shells. Changes to it get
  read with that in mind.

### Panics and error handling

A TUI that panics leaves the terminal in raw mode and the user with no prompt.

- No `unwrap()` or `expect()` on I/O, parsing, or any network result outside
  of tests.
- No index or slice that a short or empty response can push out of bounds.
- Errors surface to the user or the caller. Silently swallowing one, or
  falling back to a plausible-looking default that hides a failure, is a
  defect worth reporting.
- Terminal state is restored on every exit path, including the error paths.

### Readability

- Functions stay short enough to hold at once. A long one usually wants a
  named helper, not a comment announcing its sections.
- Names say what the value is. Rust conventions throughout: `snake_case` for
  functions, files, and variables, `PascalCase` for types, `SCREAMING_SNAKE`
  for constants.
- Report unclear naming, dead code, and duplicated logic even when the code
  is correct.

### Tests

- New behavior arrives with a deterministic test. Rendering is tested through
  Ratatui's `TestBackend`, including narrow and awkward terminal sizes.
- Missing coverage for a new branch is worth reporting on its own.
- A test that asserts on wall-clock timing or live network state is a flake.

### Cross-platform

Windows sends a key event on release as well as press, and repeats. Input
handling filters those. Rendering is not validated by CI on any platform, so
a change to layout or drawing needs the manual checklist in the README.
