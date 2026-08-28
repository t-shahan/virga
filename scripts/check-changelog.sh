#!/bin/sh
# Prove that a change describes itself in CHANGELOG.md, and that it does so in
# the section that will ship it.
#
#   ./scripts/check-changelog.sh [base-ref]      # defaults to origin/main
#
# Two contracts, because the file has two halves and they fail differently.
#
# Below the newest dated heading is history: sections whose tag exists and
# whose text people have already read on a release page. Writing there is
# always a mistake, and a quiet one — the notes look filed, the release they
# belong to gets none, and nothing notices until `release.sh` refuses to tag a
# section with a heading and no prose.
#
# Above it is the section being written. Work a user can observe has to say so
# there, or it ships undescribed.
#
# Both contracts are escapable by a commit trailer, because both have a real
# exception and neither should be argued with in a pull request comment:
#
#   Changelog: history   this change is repairing a shipped section on purpose
#   Changelog: none      this change is user-facing but wants no entry
set -eu

die() { printf 'changelog: %s\n' "$1" >&2; exit 1; }

cd "$(dirname "$0")/.."

base="${1:-origin/main}"
git rev-parse -q --verify "$base^{commit}" >/dev/null \
    || die "'$base' is not a commit this clone can see."

# The merge base, not the ref itself: on a pull request the branch is usually
# behind main, and everything main gained meanwhile is not this change's doing.
since=$(git merge-base "$base" HEAD)

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

git show "$since:CHANGELOG.md" > "$work/base" 2>/dev/null \
    || die "$since has no CHANGELOG.md to compare against."

# The newest heading carrying a date, as it stood before this change. A
# section only gets its date when `release.sh` stamps it, so this line is
# exactly the boundary between what has shipped and what is being written —
# and taking it from the base means the release commit, which dates a heading
# above this line, is not mistaken for a rewrite of history.
anchor=$(grep -E '^## \[.*\] - ' "$work/base" | head -1 || true)

trailers=$(git log --format='%B' "$since..HEAD")
trailer() { printf '%s\n' "$trailers" | grep -qiE "^Changelog:[[:space:]]*$1[[:space:]]*$"; }

# --- released sections are frozen -------------------------------------------

# From the anchor down to the link references, which are excluded: release-pr
# .yml rewrites the `[Unreleased]` line and adds one per version, and those
# live at the foot of the file inside every section's shadow.
shipped() {
    awk -v anchor="$anchor" '
        $0 == anchor { inside = 1 }
        /^\[/ && /\]: http/ { inside = 0 }
        inside
    ' "$1"
}

if [ -n "$anchor" ] && ! trailer history; then
    shipped "$work/base" > "$work/base-shipped"
    shipped CHANGELOG.md  > "$work/head-shipped"

    if ! cmp -s "$work/base-shipped" "$work/head-shipped"; then
        diff -u "$work/base-shipped" "$work/head-shipped" \
            --label "CHANGELOG.md (as released)" \
            --label "CHANGELOG.md (this change)" >&2 || true
        printf '\n' >&2
        die "the lines above sit at or below '$anchor', which has shipped and been read. Notes for work that has not shipped go above that heading; a deliberate repair says so with a 'Changelog: history' trailer."
    fi
fi

# --- user-facing work is described ------------------------------------------

subjects=$(git log --format='%s' "$since..HEAD")
bodies=$(git log --format='%b' "$since..HEAD")

# The same release types release-pr.yml counts, for the same reason: a version
# is cut for what a user can observe, so those are the commits that owe the
# changelog a line. If that list changes, change it in both places.
observable=false
if printf '%s\n' "$subjects" | grep -qE '^(feat|fix|perf)(\([^)]*\))?!?:' \
    || printf '%s\n' "$subjects" | grep -qE '^[a-zA-Z]+(\([^)]*\))?!:' \
    || printf '%s\n' "$bodies" | grep -qE '^BREAKING[ -]CHANGE:'; then
    observable=true
fi

# Everything above the anchor, minus headings, link references and blank
# lines: the prose, and nothing that a rename or a release stamp would move.
notes() {
    awk -v anchor="$anchor" '$0 == anchor { exit } { print }' "$1" \
        | sed -e '/^#/d' -e '/^\[/d' -e '/^[[:space:]]*$/d'
}

if [ "$observable" = true ] && ! trailer none; then
    if [ "$(notes "$work/base")" = "$(notes CHANGELOG.md)" ]; then
        die "This adds work a user can observe and no notes describing it. Write them under the topmost CHANGELOG.md section, or say why not with a 'Changelog: none' trailer."
    fi
fi

printf 'changelog: ok\n'
