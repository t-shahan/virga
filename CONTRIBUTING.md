# Contributing

The substance is in the README's
[Contributing](README.md#contributing) section: open an issue before a
substantial change, run the four gates before opening a pull request, keep
the template's `Review focus` section, and describe user-visible work in
[`CHANGELOG.md`](CHANGELOG.md) under the topmost section. The conventions a
review checks against are in [`CLAUDE.md`](CLAUDE.md). This file exists so
GitHub can link it from the new-pull-request page; it adds only the release
sequence, which the README describes one command of.

## The four gates

```bash
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked --all-targets
cargo package --locked
```

CI runs the same four and refuses a pull request that fails any of them.

## Cutting a release

1. Merge the `release-notes` pull request that
   [`release-pr.yml`](.github/workflows/release-pr.yml) keeps open. It
   carries the `## [X.Y.Z]` heading and the notes for what has landed since
   the last tag; read the notes as a user would before merging.
2. On an up-to-date `main` with a clean tree, run
   `./scripts/release.sh X.Y.Z`. It bumps the manifest, dates the changelog
   heading, runs the four gates, then commits, tags, and pushes after a
   `[y/N]`. If a gate fails, the bump is still in the working tree; discard
   it with `git checkout -- Cargo.toml Cargo.lock CHANGELOG.md`.
3. Watch the [release run](https://github.com/t-shahan/virga/actions/workflows/release.yml).
   It builds five platforms, publishes the release with checksums and build
   provenance, and pushes a formula to the Homebrew tap.
4. Confirm the release page lists all five archives and `SHA256SUMS`, and
   that the tap commit landed on
   [`t-shahan/homebrew-virga`](https://github.com/t-shahan/homebrew-virga).
   Without the `TAP_KEY` secret the release still publishes and only the tap
   update is skipped, with a warning in the run.
5. A prerelease (`X.Y.Z-rc1`) borrows the final version's changelog section
   and currently stamps a date on it, which freezes that section against the
   changelog gate until the final release; see
   [#66](https://github.com/t-shahan/virga/issues/66) before cutting one.

## Reporting a vulnerability

Privately, per [`SECURITY.md`](SECURITY.md).
