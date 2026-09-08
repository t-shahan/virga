#!/bin/sh
# Drive release.sh through the branches a real run only reaches by pushing a
# tag: an rc followed by the final release, the refusals, and the exits that
# leave the bump behind. These went unexercised until #66 was filed.
#
#   ./scripts/check-release.sh [shell]      # defaults to sh
#
# The argument is the interpreter release.sh and check-changelog.sh run under.
# Their shebang is /bin/sh, which is dash on Debian and bash on macOS, and the
# two differ on exactly the paths tested last: dash ends on an untrapped
# signal without running the EXIT trap, bash runs it with $? still 0. CI
# drives both.
#
# Everything happens in a throwaway repository under mktemp with its own bare
# `origin`, a stub `cargo` first on PATH, and git told to read no config from
# the machine. Nothing here can reach this repository, a real remote or the
# network.
set -eu

shell="${1:-sh}"
command -v "$shell" >/dev/null || { echo "check-release: no '$shell' on PATH" >&2; exit 1; }

cd "$(dirname "$0")/.."
src=$(pwd)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
repo="$work/repo"
origin="$work/origin.git"

# ~/.gitconfig could sign tags, run hooks or alias `push`. None of that
# belongs in a test, and neither does the machine's default branch name.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=check-release GIT_AUTHOR_EMAIL=check-release@example.invalid
export GIT_COMMITTER_NAME=check-release GIT_COMMITTER_EMAIL=check-release@example.invalid

# The last run's output, so a red build says what release.sh said.
fail() {
    printf 'check-release: %s\n--- release.sh stdout:\n' "$1" >&2
    cat "$work/out" >&2
    printf -- '--- release.sh stderr:\n' >&2
    cat "$work/err" >&2
    exit 1
}

# --- the throwaway repository -----------------------------------------------

mkdir -p "$repo/scripts" "$work/bin"
cp "$src/scripts/release.sh" "$src/scripts/check-changelog.sh" "$repo/scripts/"

# Every gate passes unless the harness names one. STUB_CARGO_FAIL makes that
# subcommand exit 101, which is what a failing `cargo test` returns.
# STUB_CARGO_SIGNAL_ON names the subcommand during which STUB_CARGO_SIGNAL is
# sent to release.sh's shell and to the stub itself, which is what Ctrl-C does
# to a foreground process group. A stub that survives its own signal reports
# the environment rather than passing: a `&` job in a non-interactive shell
# inherits SIGINT ignored, and so would everything under it.
cat > "$work/bin/cargo" <<'STUB'
#!/bin/sh
if [ "$1" = "${STUB_CARGO_FAIL:-}" ]; then
    exit 101
fi
if [ "$1" = "${STUB_CARGO_SIGNAL_ON:-}" ]; then
    kill -s "$STUB_CARGO_SIGNAL" "$PPID" $$
    echo "stub cargo: SIG$STUB_CARGO_SIGNAL was ignored; run the harness from a foreground shell" >&2
    exit 99
fi
exit 0
STUB
chmod +x "$work/bin/cargo"

printf '[package]\nname = "virga"\nversion = "0.6.1"\nedition = "2021"\n' > "$repo/Cargo.toml"
printf 'version = 4\n\n[[package]]\nname = "virga"\nversion = "0.6.1"\n' > "$repo/Cargo.lock"
# The shape of the real file: an open section, a shipped one, link references
# at the foot. The link references matter because check-changelog.sh takes
# them as the end of the shipped range.
cat > "$repo/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

## [0.6.2]

### Fixed

- The rc borrows this section.

## [0.6.1] - 2026-08-01

### Added

- Shipped already.

[Unreleased]: https://example.invalid/compare/v0.6.2...HEAD
[0.6.2]: https://example.invalid/compare/v0.6.1...v0.6.2
[0.6.1]: https://example.invalid/releases/tag/v0.6.1
EOF

git init -q "$repo"
git -C "$repo" symbolic-ref HEAD refs/heads/main
git init -q --bare "$origin"
git -C "$repo" remote add origin "$origin"
git -C "$repo" add -A
git -C "$repo" commit -q -m 'chore: seed'
git -C "$repo" push -q -u origin main

# --- helpers ----------------------------------------------------------------

# release <version> [VAR=value ...]: run release.sh with --yes and the given
# stub settings; the exit status lands in $status, the output in $work.
release() {
    version=$1
    shift
    status=0
    (cd "$repo" && env PATH="$work/bin:$PATH" "$@" "$shell" scripts/release.sh "$version" --yes) \
        >"$work/out" 2>"$work/err" || status=$?
}

# commit <subject>, then push: release.sh refuses a main that has diverged
# from origin, so every commit is pushed before the next run. The two are
# separate because check-changelog.sh measures HEAD against origin/main and
# would have nothing to judge once they match.
commit() {
    git -C "$repo" add -A
    git -C "$repo" commit -q -m "$1"
}
push() { git -C "$repo" push -q origin main 2>/dev/null; }

# Portable in-place edits: sed -i differs between GNU and BSD.
replace_line() {
    awk -v from="$2" -v to="$3" '$0 == from { print to; next } { print }' "$1" > "$1.new" \
        && mv "$1.new" "$1"
}
insert_after() {
    awk -v after="$2" -v text="$3" '{ print } !done && $0 == after { print ""; print text; done = 1 }' "$1" > "$1.new" \
        && mv "$1.new" "$1"
}

manifest_version() { sed -n 's/^version = "\(.*\)"$/\1/p' "$repo/Cargo.toml" | head -1; }
has_tag() { git -C "$1" rev-parse -q --verify "refs/tags/v$2" >/dev/null; }
clean() { git -C "$repo" diff --quiet; }
hinted() { grep -q 'bump is still in your working tree' "$work/err"; }

expect_status() { [ "$status" -eq "$1" ] || fail "$2: release.sh exited $status, want $1"; }
expect_hint() { hinted || fail "$1: no undo hint on stderr"; }
expect_no_hint() { ! hinted || fail "$1: undo hint on stderr, but nothing was bumped"; }
expect_clean() { clean || fail "$1: the working tree was touched"; }
expect_no_tag() { ! has_tag "$repo" "$2" || fail "$1: v$2 was tagged"; }

# The hint's own command must be enough to get back to a clean tree, and the
# tree must actually have been dirty for that to prove anything.
expect_bump_left_behind() {
    ! clean || fail "$1: the working tree is clean; the bump should still be in it"
    [ "$(manifest_version)" = "$2" ] || fail "$1: Cargo.toml says $(manifest_version), want the bumped $2"
    grep -q '^  git checkout -- Cargo.toml Cargo.lock CHANGELOG.md$' "$work/err" \
        || fail "$1: the hint does not name the undo command"
    git -C "$repo" checkout -q -- Cargo.toml Cargo.lock CHANGELOG.md
    expect_clean "$1 (after the hinted checkout)"
}

# --- an rc leaves the heading undated and tags ------------------------------

release 0.6.2-rc1
expect_status 0 "rc"
expect_clean "rc"
[ "$(manifest_version)" = "0.6.2-rc1" ] || fail "rc: Cargo.toml says $(manifest_version)"
grep -qx '## \[0\.6\.2\]' "$repo/CHANGELOG.md" || fail "rc: the [0.6.2] heading is no longer bare"
has_tag "$repo" 0.6.2-rc1 || fail "rc: no local tag"
has_tag "$origin" 0.6.2-rc1 || fail "rc: the tag was not pushed"
[ "$(git -C "$origin" rev-parse main)" = "$(git -C "$repo" rev-parse HEAD)" ] \
    || fail "rc: the release commit was not pushed"

# --- the borrowed section stays open to ordinary edits ----------------------

replace_line "$repo/CHANGELOG.md" \
    '- The rc borrows this section.' \
    '- The rc borrowed this section; a fix landed after it and rewords the note.'
commit 'fix: reword the 0.6.2 notes after the rc'
(cd "$repo" && "$shell" scripts/check-changelog.sh origin/main) >"$work/out" 2>"$work/err" \
    || fail "edit after rc: check-changelog.sh refused an edit to the unshipped section"
push

# --- the final release stamps the date --------------------------------------

release 0.6.2
expect_status 0 "final"
expect_clean "final"
[ "$(manifest_version)" = "0.6.2" ] || fail "final: Cargo.toml says $(manifest_version)"
grep -qE '^## \[0\.6\.2\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$' "$repo/CHANGELOG.md" \
    || fail "final: the [0.6.2] heading was not dated"
grep -qx '## \[0\.6\.1\] - 2026-08-01' "$repo/CHANGELOG.md" \
    || fail "final: the shipped [0.6.1] heading changed"
has_tag "$origin" 0.6.2 || fail "final: the tag was not pushed"

# --- a section holding only a sub-heading is refused before the bump --------

insert_after "$repo/CHANGELOG.md" '## [Unreleased]' '## [0.6.3]'
insert_after "$repo/CHANGELOG.md" '## [0.6.3]' '### Fixed'
commit 'docs: open the 0.6.3 changelog section'
push

release 0.6.3
expect_status 1 "bare sub-heading"
grep -q 'has a heading but no notes' "$work/err" || fail "bare sub-heading: no refusal on stderr"
expect_no_hint "bare sub-heading"
expect_clean "bare sub-heading"
expect_no_tag "bare sub-heading" 0.6.3
[ "$(manifest_version)" = "0.6.2" ] || fail "bare sub-heading: Cargo.toml was bumped to $(manifest_version)"

# --- a failing gate leaves the bump behind and says so ----------------------

insert_after "$repo/CHANGELOG.md" '### Fixed' '- A note under the sub-heading.'
commit 'docs: write the 0.6.3 notes'
push

release 0.6.3 STUB_CARGO_FAIL=test
expect_status 101 "failed gate"
expect_hint "failed gate"
expect_no_tag "failed gate" 0.6.3
expect_bump_left_behind "failed gate" 0.6.3

# --- so does an interrupted gate --------------------------------------------

release 0.6.3 STUB_CARGO_SIGNAL_ON=test STUB_CARGO_SIGNAL=INT
expect_status 130 "SIGINT during a gate"
expect_hint "SIGINT during a gate"
expect_no_tag "SIGINT during a gate" 0.6.3
expect_bump_left_behind "SIGINT during a gate" 0.6.3

release 0.6.3 STUB_CARGO_SIGNAL_ON=test STUB_CARGO_SIGNAL=TERM
expect_status 143 "SIGTERM during a gate"
expect_hint "SIGTERM during a gate"
expect_no_tag "SIGTERM during a gate" 0.6.3
expect_bump_left_behind "SIGTERM during a gate" 0.6.3

# --- and a successful run says nothing extra --------------------------------

release 0.6.3
expect_status 0 "final after a failed gate"
expect_no_hint "final after a failed gate"
expect_clean "final after a failed gate"
has_tag "$origin" 0.6.3 || fail "final after a failed gate: the tag was not pushed"

echo "release script ($shell): ok"
