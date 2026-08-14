# Virga Portfolio README Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize Virga's README into a polished portfolio page that still encourages installation, use, and focused community contributions.

**Architecture:** Keep `README.md` as the single public project guide, preserving its two animated demonstrations and legally required data attribution. Reorder and tighten the existing material so the product story, maintenance status, architecture, and engineering quality are clear before the detailed operating reference.

**Tech Stack:** Markdown, Mermaid, Rust/Cargo, Ratatui, Open-Meteo, GitHub Actions

## Global Constraints

- Present Virga as feature-complete and not actively maintained; contributions remain welcome, but review timing may vary.
- Preserve both existing GIF asset URLs exactly and keep the main-interface GIF before the precipitation-interface GIF.
- Preserve the CI badge and dual `MIT OR Apache-2.0` badge and licensing terms.
- Preserve Open-Meteo and CAMS attribution, the free-service non-commercial-use warning, and the stated rate limit.
- State that manual terminal testing is limited to Ghostty and Apple's Terminal app on macOS; Linux and Windows coverage is automated CI only.
- Require Rust 1.88 or later and do not change application code, dependencies, behavior, or release state.
- Use commit `980d9a4` as the public README content baseline.

---

### Task 1: Rewrite the Public README

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: Existing screenshots, installation commands, controls, theme configuration, CI workflow, module structure, provider terms, and license files.
- Produces: A self-contained GitHub project page for recruiters, users, and contributors.

- [ ] **Step 1: Record the verified test and CI evidence**

Run:

```bash
cargo test --locked --all-targets
```

Expected: all non-ignored tests pass. Use the exact passing-test total shown by Cargo if the README includes a numeric test claim; otherwise use the accurate phrase "hundreds of deterministic tests." Do not count ignored live-API tests as passing deterministic tests.

Inspect CI and the source boundaries:

```bash
sed -n '1,260p' .github/workflows/ci.yml
find src -maxdepth 2 -type f | sort
```

Expected: CI shows format, lint, Linux/macOS/Windows tests, Rust 1.88 MSRV, package, and audit jobs; source files show separate `ui` and `weather` modules.

- [ ] **Step 2: Replace the opening with portfolio positioning**

Keep the existing CI and license badges. Use a concise opening that identifies Virga as a responsive Rust terminal weather application with current conditions, multi-day forecasts, historical context, and hourly precipitation visualization, powered by Open-Meteo without an account or API key.

Immediately follow it with this status meaning, written naturally rather than as a warning:

```markdown
> **Project status: Feature-complete.** Virga is not under active maintenance,
> but it remains available to install and use. Focused contributions are
> welcome; reviews and responses may take time.
```

Retain the short explanation of the name *Virga*.

- [ ] **Step 3: Preserve and frame both demonstrations**

Keep these image URLs unchanged and in this order:

```text
https://github.com/user-attachments/assets/0a773e11-df73-4cc3-9a75-f3bad3cbc727
https://github.com/user-attachments/assets/9f61e32f-d342-4794-b64b-7d1e6efb0a97
```

Place the first beneath the opening as the main weather-interface demo. Introduce the second as the hourly precipitation view and retain the explanation that probability rises above the center rule while forecast amount hangs below it.

- [ ] **Step 4: Reorganize features and installation**

Use a compact `Highlights` section covering current conditions, eight-day forecast, three-week historical/forecast context, hourly precipitation, day browsing, city search, unit switching, five themes, and 34×12 responsive behavior.

Follow it with `Install` and preserve:

```bash
cargo install --git https://github.com/t-shahan/virga
virga
```

Keep Rust 1.88, Unicode-terminal, internet-connection, release-build, update, and `cargo uninstall virga-tui` guidance. Remove the unsupported sentence promising additional installation methods.

- [ ] **Step 5: Add the architecture and engineering-quality story**

Add an `Architecture` section with a small Mermaid flowchart representing:

```text
Keyboard events -> event/input handling -> application state -> Ratatui UI
Open-Meteo APIs -> HTTP client -> DTO conversion -> domain model -> application state
Remembered location <-> application state
```

Explain that `src/ui/` performs no networking, `src/weather/client.rs` owns HTTP requests, `src/weather/dto.rs` isolates provider wire formats, and `src/weather/model.rs` supplies stable domain data.

Add an `Engineering Quality` section that accurately covers:

- deterministic Ratatui `TestBackend` rendering checks;
- navigation and unit-conversion boundary tests;
- null, malformed, and mismatched API response coverage;
- loopback-server timeout tests;
- Linux, macOS, and Windows CI;
- rustfmt, Clippy with warnings denied, Rust 1.88 MSRV, package-content checks, and pinned dependency auditing.

Use the exact test total from Step 1 only if it is unambiguous.

- [ ] **Step 6: Preserve the operating reference and legal detail**

Keep the controls table and precipitation-chart legend explanation. Retain the five-theme table, foreground-only design rationale, truecolor caveat, `VIRGA_THEME` examples, remembered-location behavior, limitations, data/privacy behavior, Open-Meteo terms and license links, full CAMS citation, and dual-license terms.

Update the manual-testing limitation to say exactly that Ghostty and Apple's Terminal app have been tested manually on macOS. State separately that automated tests run on Linux, macOS, and Windows and do not validate real-terminal rendering, font fallback, or held-key behavior.

- [ ] **Step 7: Add contribution guidance**

Add a `Contributing` section before `Limitations`. Invite focused bug fixes, documentation improvements, tests, accessibility work, and well-scoped features. Ask contributors to open an issue before substantial changes and disclose that review timing may vary because the project is not actively maintained.

List the local verification commands:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

- [ ] **Step 8: Inspect the README diff**

Run:

```bash
git diff --check
git diff -- README.md
```

Expected: no whitespace errors; only the approved restructuring and copy changes appear in `README.md`; both GIFs and all attribution/licensing content remain.

- [ ] **Step 9: Commit the README rewrite**

```bash
git add README.md
git commit -m "docs: present Virga as a portfolio project"
```

### Task 2: Verify the Portfolio README

**Files:**
- Verify: `README.md`
- Verify: `Cargo.toml`
- Verify: `.github/workflows/ci.yml`
- Verify: `LICENSE-MIT`
- Verify: `LICENSE-APACHE`

**Interfaces:**
- Consumes: The completed `README.md` from Task 1 and repository source-of-truth files.
- Produces: Evidence that the public documentation is accurate, complete, and ready to push.

- [ ] **Step 1: Verify protected content and status wording**

Run:

```bash
rg -n "0a773e11-df73-4cc3-9a75-f3bad3cbc727|9f61e32f-d342-4794-b64b-7d1e6efb0a97|Feature-complete|not under active maintenance|Ghostty|Apple's Terminal|Open-Meteo|CAMS|MIT OR Apache" README.md
```

Expected: both GIF URLs appear once, in the required order; maintenance and manual-testing language is explicit; attribution and dual licensing remain present.

- [ ] **Step 2: Verify commands and repository facts**

Run:

```bash
rg -n "rust-version|name = \"virga-tui\"|name = \"virga\"" Cargo.toml
rg -n "ubuntu-latest|macos-latest|windows-latest|1\.88\.0|cargo audit|cargo package" .github/workflows/ci.yml
rg -n "cargo install --git|cargo uninstall virga-tui|cargo test --locked --all-targets|cargo clippy --all-targets --locked" README.md
```

Expected: the documented Rust floor, executable/package names, install/uninstall commands, and CI claims match their source files.

- [ ] **Step 3: Run final project verification**

Run:

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

Expected: every command exits successfully; ignored live-API tests remain excluded from the standard test result.

- [ ] **Step 4: Confirm the final Git state**

Run:

```bash
git diff --check HEAD~1..HEAD
git status --short --branch
git log -3 --oneline
```

Expected: the README commit contains no whitespace errors, the worktree is clean, and the local branch contains the design-spec, clarification, and README commits on top of `980d9a4`.

