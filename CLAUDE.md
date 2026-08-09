# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

`weather_tui` is a **bare `cargo new` scaffold** — `src/main.rs` is still the generated hello-world, `[dependencies]` is empty, and the repo has **no commits yet** (branch `master`, no `main`). There is no architecture to preserve; the intended product (per the directory name `weatherTUI`) is a terminal weather UI, but nothing about it has been decided in code. Do not infer design constraints from files that do not exist — ask, or propose the structure explicitly.

## Commands

```bash
cargo run                  # build + run the binary
cargo build --release      # optimized build → target/release/weather_tui
cargo check                # fast type/borrow check, no codegen
cargo fmt                  # rustfmt (no rustfmt.toml — stock defaults)
cargo clippy -- -D warnings
cargo test                 # whole suite
cargo test <name>          # tests whose path contains <name>
cargo test <name> -- --exact --nocapture   # one test, with stdout shown
```

## Toolchain

`Cargo.toml` sets `edition = "2024"`, which requires Rust ≥ 1.85. There is no `rust-toolchain.toml`, so builds use whatever is on `PATH` (currently 1.97.1). Pin a toolchain file if edition-2024 support ever needs to be guaranteed for other machines.

The intended architecture is written up in [DESIGN.md](DESIGN.md) — module layout, core types, threading model, and build order. **The owner is implementing this themselves to learn Rust.** Do not write implementation code for them unless they explicitly ask; help by reviewing, explaining compiler errors, and answering design questions.

## Notes for building this out

- TUI work is a runtime + terminal-backend decision that is hard to reverse — settle on the crate stack (e.g. `ratatui` + `crossterm`, and whether an async runtime is needed for HTTP) before writing much beyond `main.rs`.
- A TUI cannot be verified by reading test output alone. Anything touching rendering or key handling should be exercised with `cargo run` in a real terminal; keep the weather-fetch and formatting layers free of terminal I/O so they stay unit-testable.
- Weather APIs need a key or at minimum a caller identity. Read it from the environment, never from a committed file — `.gitignore` currently ignores only `/target`.
