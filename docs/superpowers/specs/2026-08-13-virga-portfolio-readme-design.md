# Virga Portfolio README Design

## Objective

Rework Virga's README into a portfolio-first project page that remains useful
to people who want to install, use, or contribute to the application. Present
Virga as feature-complete and not actively maintained, while making clear that
contributions are welcome and may be reviewed as the maintainer's time allows.

## Audience and Positioning

The README serves two primary audiences:

1. Recruiters and engineers evaluating the project's design, testing, and
   delivery quality.
2. Terminal users and prospective contributors who want to install, operate,
   or extend Virga.

The opening should describe Virga as a responsive Rust terminal weather
application powered by Open-Meteo, with no account or API key required. A
visible status callout will say that the project is complete, is not under
active maintenance, and welcomes contributions with potentially delayed
responses. It must not call the project decommissioned because the application
remains installable and usable.

## Content Structure

The README will use this hierarchy:

1. Project title, CI and license badges, and a concise value proposition.
2. Maintenance-status callout.
3. Both existing animated GIF demonstrations, prominently displayed and kept
   in their current order:
   - Main weather interface:
     `https://github.com/user-attachments/assets/0a773e11-df73-4cc3-9a75-f3bad3cbc727`
   - Hourly precipitation interface:
     `https://github.com/user-attachments/assets/9f61e32f-d342-4794-b64b-7d1e6efb0a97`
4. A compact feature overview explaining the user-facing value.
5. Quick installation and a concise controls reference.
6. An architecture section showing the event loop, Open-Meteo clients,
   wire-to-domain conversion, application state, and Ratatui rendering flow.
7. An engineering-quality section highlighting responsive layouts, DTO/domain
   separation, deterministic rendering and navigation tests, timeout and
   malformed-response coverage, cross-platform CI, MSRV enforcement,
   packaging, and dependency audits.
8. Themes and configuration.
9. Contribution guidance calibrated to the maintenance status.
10. Limitations, data/privacy behavior, required attribution, and licensing.

The existing operational detail will be retained where it helps users or
accurately communicates engineering judgment. Repetition and explanatory
passages that obscure the main story may be tightened.

## Architecture Narrative

The architecture section will describe responsibility boundaries already
present in the codebase:

- `src/weather/client.rs` performs weather, air-quality, and geocoding HTTP
  requests.
- `src/weather/dto.rs` models provider responses and converts wire data into
  stable domain types in `src/weather/model.rs`.
- Application state and input/event handling manage navigation, search,
  refresh, units, themes, and remembered location.
- `src/ui/` renders state with Ratatui and performs no networking.

A small Mermaid diagram will visualize the data flow without duplicating the
module list. The diagram is intended to make separation of concerns legible to
portfolio reviewers at a glance.

## Installation and Contribution Experience

The current `cargo install --git` instructions, Rust 1.88 minimum, release-run
guidance, package-name uninstall detail, key table, theme behavior, and
`VIRGA_THEME` configuration will remain accurate. The unsupported promise that
more installation methods are coming will be removed.

The contribution section will invite focused bug fixes, documentation
improvements, tests, accessibility improvements, and well-scoped features. It
will ask contributors to open an issue before substantial work and state that
review timing may vary. Verification commands will match CI:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

## Accuracy and Preservation Requirements

- Preserve both animated GIFs exactly; neither may be replaced, removed, or
  demoted to a collapsed section.
- Preserve the CI badge and dual MIT/Apache-2.0 license badge.
- Preserve Open-Meteo and CAMS attribution and distinguish application-source
  licensing from fetched-data licensing.
- Preserve the non-commercial-use and rate-limit warning for Open-Meteo's free
  service.
- Do not claim that terminal rendering has been manually validated on Linux or
  Windows.
- Use the current GitHub README at commit `980d9a4` as the content baseline.
- Do not change application code, behavior, dependencies, or release state.

## Verification

The finished README will be checked for:

- Presence and order of both GIF asset URLs.
- Correct maintenance-status wording.
- Valid internal file links and section anchors.
- Commands, Rust version, API facts, CI claims, and test-count claims against
  repository evidence.
- Clean Markdown whitespace and a README-only implementation diff, apart from
  the committed design and implementation-plan documents required by the
  workflow.

