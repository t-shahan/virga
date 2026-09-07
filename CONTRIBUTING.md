# Contributing

The substance is in the README's
[Contributing](README.md#contributing) section: open an issue before a
substantial change, run the four gates before opening a pull request, keep
the template's `Review focus` section, and describe user-visible work in
[`CHANGELOG.md`](CHANGELOG.md) under the topmost section. The conventions a
review checks against are in [`CLAUDE.md`](CLAUDE.md). This file exists so
GitHub can link it from the new-pull-request page; it adds only the steps
around `release.sh` that the README leaves out.

## Cutting a release

1. Find the `release-notes` pull request that
   [`release-pr.yml`](.github/workflows/release-pr.yml) opens once a
   `feat`, `fix`, `perf` or breaking change (a `!` after the type, or a
   `BREAKING CHANGE:` footer) has landed since the last full release. It
   opens an empty `## [X.Y.Z]` section under `## [Unreleased]` and lists
   those commits in its body. Write the notes into the branch as a user
   would read them, then merge. A release with only `docs` or `chore`
   commits gets no such pull request, so open the section by hand.
2. On an up-to-date `main` with a clean tree, run
   `./scripts/release.sh X.Y.Z`; the README's
   [Cutting a release](README.md#cutting-a-release) says what it does. It
   asks `[y/N]` before it commits, tags and pushes (`--yes` skips the
   question). If a gate fails, or you answer `N`, the version bump is still
   in the working tree; discard it with
   `git checkout -- Cargo.toml Cargo.lock CHANGELOG.md`.
3. Watch the [release run](https://github.com/t-shahan/virga/actions/workflows/release.yml).
4. Confirm the release page lists all five archives and `SHA256SUMS`. For a
   full release with the `TAP_KEY` secret set, confirm too that a
   `virga X.Y.Z` commit landed on
   [`t-shahan/homebrew-virga`](https://github.com/t-shahan/homebrew-virga).

A prerelease (`X.Y.Z-rc1`) never touches the tap. It also borrows the final
version's changelog section and currently stamps a date on it, which the
changelog gate then treats as shipped, so every edit to those notes before
the final release needs a `Changelog: history` trailer; see
[#66](https://github.com/t-shahan/virga/issues/66) before cutting one.

## Reporting a vulnerability

Privately, per [`SECURITY.md`](SECURITY.md).
