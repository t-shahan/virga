#!/bin/sh
# Cut a release.
#
#   ./scripts/release.sh 0.2.0
#
# Bumps the manifest, runs the same gates CI runs, then commits, tags and
# pushes. Everything after the tag push is automatic: .github/workflows/
# release.yml builds five platforms, publishes the release with checksums and
# provenance, and updates the Homebrew tap.
#
# The checks below run before the tag exists on purpose. A tag is the one thing
# here that is awkward to take back once it has been pushed, so nothing should
# reach that point that a local `cargo test` would have caught.
set -eu

BRANCH=main

die() { printf 'release: %s\n' "$1" >&2; exit 1; }
step() { printf '\n==> %s\n' "$1"; }

version="${1:-}"
assume_yes=false
[ "${2:-}" = "--yes" ] && assume_yes=true

case "$version" in
    '') die "Usage: ./scripts/release.sh <version> [--yes]   (e.g. 0.2.0)" ;;
esac
printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$' \
    || die "'$version' is not MAJOR.MINOR.PATCH[-prerelease]."

cd "$(dirname "$0")/.."

# --- the repository is in a releasable state --------------------------------

step "checking the working tree"

[ "$(git rev-parse --abbrev-ref HEAD)" = "$BRANCH" ] \
    || die "Releases are cut from $BRANCH, not $(git rev-parse --abbrev-ref HEAD)."

git diff --quiet && git diff --cached --quiet \
    || die "The working tree has uncommitted changes."

git fetch --quiet origin "$BRANCH"
[ "$(git rev-parse HEAD)" = "$(git rev-parse "origin/$BRANCH")" ] \
    || die "$BRANCH and origin/$BRANCH have diverged. Pull or push first."

git rev-parse -q --verify "refs/tags/v$version" >/dev/null \
    && die "Tag v$version already exists."

# The release workflow refuses a version the changelog does not describe. Fail
# here instead, where it costs a moment rather than a failed CI run.
release="${version%%-*}"
grep -q "^## \[$release\]" CHANGELOG.md \
    || die "CHANGELOG.md has no '## [$release]' section. Write the notes first."

# --- bump ------------------------------------------------------------------

step "setting the version to $version"

# Only the first `version =`, which is the package's own. Dependency versions
# further down the manifest must not be touched.
awk -v v="$version" '
    !done && /^version = / { print "version = \"" v "\""; done = 1; next }
    { print }
' Cargo.toml > Cargo.toml.new && mv Cargo.toml.new Cargo.toml

# Refreshes Cargo.lock's entry for this package. Deliberately without
# --locked, because the lock file is exactly what is out of date right now.
cargo check --quiet

# Today's date, so a section written days ago does not ship with a stale one.
today=$(date -u +%Y-%m-%d)
awk -v want="## [$release]" -v date="$today" '
    !done && index($0, want) == 1 { print want " - " date; done = 1; next }
    { print }
' CHANGELOG.md > CHANGELOG.new && mv CHANGELOG.new CHANGELOG.md

# --- the same gates CI runs -------------------------------------------------

step "cargo fmt --check";                                   cargo fmt --check
step "cargo clippy";  cargo clippy --all-targets --locked -- -D warnings
step "cargo test";                     cargo test --locked --all-targets
step "cargo package";                             cargo package --locked

# --- ship -------------------------------------------------------------------

step "ready to release v$version"
git --no-pager diff --stat

if [ "$assume_yes" != true ]; then
    # A pushed tag is the one irreversible step here, and it is what starts the
    # build, the release and the tap update. Worth a keystroke.
    printf '\nPush v%s and start the release? [y/N] ' "$version"
    read -r answer </dev/tty
    case "$answer" in
        y|Y|yes|Yes) ;;
        *) die "Stopped. The version bump is still in your working tree." ;;
    esac
fi

git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): v$version"
git tag -a "v$version" -m "virga v$version"
git push origin "$BRANCH"
git push origin "v$version"

printf '\nPushed v%s. Watch the build:\n  gh run watch --repo %s\n' \
    "$version" "$(git remote get-url origin | sed 's#.*github.com[:/]##; s#\.git$##')"
