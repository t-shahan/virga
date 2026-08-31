---
name: virga-conventions
description: Development conventions and patterns for virga, a Rust terminal weather application.
---

# Virga conventions

The conventions themselves live in [CLAUDE.md](../../../CLAUDE.md) at the
repository root, which is also what the pull request review enforces. This
file orients; that file governs. When the two disagree, CLAUDE.md is right.

## When to use this skill

- Making changes to this repository
- Adding features that should match existing patterns
- Writing tests
- Writing commit messages

## Stack

A single Rust binary crate, `virga-tui`, producing the `virga` binary. Edition
2024, minimum supported Rust 1.89. Ratatui draws the interface, `ureq` fetches
from Open-Meteo, and `serde` parses the responses.

Layout: `src/ui/` renders, `src/weather/` fetches and parses, and the files at
the top of `src/` hold the app loop, input, state, CLI, and units.

## Code style

Standard Rust naming, which is what `cargo fmt` and Clippy already enforce:

| Element | Convention |
|---------|------------|
| Files and modules | `snake_case` |
| Functions and variables | `snake_case` |
| Types, traits, enum variants | `PascalCase` |
| Constants and statics | `SCREAMING_SNAKE_CASE` |

Comments explain why, not what. The ones worth writing record a constraint, a
rejected alternative, or a trap. The header of `.github/workflows/ci.yml` and
the `rust-version` note in `Cargo.toml` set the register.

## Tests

Unit tests live beside the code they cover, in an inline `#[cfg(test)] mod
tests` block, which is most of `src/`. `tests/fixtures/` holds recorded API
payloads, not test functions.

Rendering is tested through Ratatui's `TestBackend`, including narrow and
awkward terminal sizes. Tests that reach a live provider are marked
`#[ignore]`, so a provider outage never fails a pull request.

## Checks

The four gates CI enforces, and the four `scripts/release.sh` runs before it
will tag:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

## Commits

Conventional commits in the imperative mood. The prefixes this repository
uses: `feat`, `fix`, `perf`, `docs`, `test`, `ci`, `chore`, `refactor`.

```text
feat: default startup weather to New York City
fix: scroll the forecast table back to a selected past day
test: cover remembered-location persistence warnings
```

Work a user can observe also owes a line in `CHANGELOG.md` under the topmost
section. `scripts/check-changelog.sh` gates it.
