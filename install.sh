#!/bin/sh
# Install Virga from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/t-shahan/virga/main/install.sh | sh
#
# Re-running this is the upgrade path: it always resolves the newest release
# and overwrites in place.
#
#   VIRGA_VERSION       tag to install, e.g. v0.2.0 (default: the latest release)
#   VIRGA_INSTALL_DIR   where to put the binary (default: ~/.local/bin)
#
# POSIX sh on purpose. The one thing this script must never do is leave a
# half-written binary on PATH, so everything happens in a temporary directory
# and the final move is the only thing that touches the install directory.
set -eu

REPO="t-shahan/virga"
INSTALL_DIR="${VIRGA_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf 'virga: %s\n' "$1"; }
die() { printf 'virga: %s\n' "$1" >&2; exit 1; }

need() {
    command -v "$1" >/dev/null 2>&1 || die "$1 is required but was not found."
}

# --- what are we running on -------------------------------------------------

detect_target() {
    os=$(uname -s)
    arch=$(uname -m)

    case "$os" in
        Linux) ;;
        Darwin) ;;
        # Guessing here would install a binary that cannot run. Naming the
        # alternative is more use than a generic failure.
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            die "Windows is not covered by this script. Download the .zip from https://github.com/$REPO/releases/latest" ;;
        *)
            die "Unsupported operating system: $os. Build from source with: cargo install --git https://github.com/$REPO" ;;
    esac

    case "$arch" in
        x86_64|amd64) arch=x86_64 ;;
        aarch64|arm64) arch=aarch64 ;;
        *) die "Unsupported architecture: $arch. Build from source with: cargo install --git https://github.com/$REPO" ;;
    esac

    if [ "$os" = Darwin ]; then
        printf '%s-apple-darwin' "$arch"
    else
        printf '%s-unknown-linux-musl' "$arch"
    fi
}

# --- fetching ---------------------------------------------------------------

# Every request goes through here. A connection that stalls after the
# handshake would otherwise hang the install with no output and no prompt,
# and pinning the protocol keeps a redirect from stepping down to plain http
# on the way to the archive.
fetch() {
    curl -fsSL --proto '=https' --proto-redir '=https' --tlsv1.2 \
        --connect-timeout 15 --max-time 300 "$@"
}

# --- which release ----------------------------------------------------------

latest_tag() {
    # The redirect on /releases/latest carries the tag, which avoids both the
    # JSON parsing and the lower rate limit of the API.
    fetch -I -o /dev/null -w '%{url_effective}' \
        "https://github.com/$REPO/releases/latest" \
        | sed 's#.*/tag/##'
}

# Why SHA256SUMS could not be fetched for the tag the user named. A tag that
# does not exist and one that predates prebuilt binaries both 404 on the
# asset, and both used to be reported as "No such archive", which says
# nothing about which of the two the user should fix.
missing_sums() {
    status=$1
    base=$2
    tag=$3
    # curl exits 22 for an HTTP error and something else for a connection
    # that never answered; only the former says anything about the release.
    [ "$status" -eq 22 ] || die "Could not download $base/SHA256SUMS"
    # The tag page's status code, not the probe's exit status: -f exits 22
    # for a 403 or a 429 as readily as for a 404, and only the 404 means the
    # tag is missing. Telling a rate limit "there is no such tag" would be the
    # same false statement this function exists to remove. %{http_code} is
    # written even when -f fails, and reads 000 when nothing answered.
    code=$(fetch -I -o /dev/null -w '%{http_code}' \
        "https://github.com/$REPO/releases/tag/$tag" 2>/dev/null) || :
    case "$code" in
        200) die "Release $tag has no SHA256SUMS, so nothing in it can be verified. Prebuilt binaries start at v0.2.0." ;;
        404) die "There is no release tagged $tag. The ones that exist are listed at https://github.com/$REPO/releases" ;;
        000|'') die "Could not download $base/SHA256SUMS, and could not check whether $tag exists: github.com did not answer." ;;
        *) die "Could not download $base/SHA256SUMS, and could not check whether $tag exists: github.com answered $code." ;;
    esac
}

# --- checksums --------------------------------------------------------------

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | cut -d' ' -f1
    else
        die "Neither sha256sum nor shasum is available, so the download cannot be verified."
    fi
}

# --- do it ------------------------------------------------------------------

need curl
need tar

target=$(detect_target)
tag="${VIRGA_VERSION:-$(latest_tag)}"
[ -n "$tag" ] || die "Could not work out the latest release. Set VIRGA_VERSION to a tag."
# Release tags carry a leading v. Accept VIRGA_VERSION either way rather than
# building a URL that 404s on a difference the user cannot see.
case "$tag" in v*) ;; *) tag="v$tag" ;; esac

version="${tag#v}"
archive="virga-${version}-${target}.tar.gz"
base="https://github.com/$REPO/releases/download/$tag"

work=$(mktemp -d)
staged=
cleanup() {
    rm -rf "$work"
    [ -z "$staged" ] || rm -f "$staged"
}
# Runs on success and on failure, so a network error leaves nothing behind.
# INT and TERM turn into an exit rather than cleaning up themselves: a trap
# that only removes files returns to the line after the one the signal
# interrupted, and the script carries on to chmod and rename a staged file
# that is truncated or already gone. Leaving through the EXIT trap is what
# makes the cleanup the last thing that runs.
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# SHA256SUMS first, because it answers two questions the archive alone cannot:
# whether the release exists, and whether it was built for this platform.
status=0
fetch "$base/SHA256SUMS" -o "$work/SHA256SUMS" || status=$?
[ "$status" -eq 0 ] || missing_sums "$status" "$base" "$tag"

expected=$(grep " ${archive}\$" "$work/SHA256SUMS" | cut -d' ' -f1)
[ -n "$expected" ] || die "Release $tag has no build for $target: SHA256SUMS does not list $archive."

say "downloading $archive"
fetch "$base/$archive" -o "$work/$archive" \
    || die "Could not download $base/$archive"

actual=$(sha256_of "$work/$archive")
if [ "$expected" != "$actual" ]; then
    die "Checksum mismatch for $archive.
  expected $expected
  actual   $actual
Nothing was installed."
fi
say "checksum verified"

tar -xzf "$work/$archive" -C "$work"
binary=$(find "$work" -type f -name virga | head -n 1)
[ -n "$binary" ] || die "The archive did not contain a virga binary."

# The checksum above proves the download is intact; provenance proves who
# built it. `gh attestation verify` needs an authenticated gh to reach the
# API, so it runs when that is available and says so when it is not — a
# quiet skip would look like a pass.
#
# gh exits non-zero for a rate limit or an unreachable API as readily as for
# a binary nothing signed, and only the latter is a statement about the
# binary. Its verdict is told apart by the message, which is the only place
# gh makes the distinction; the alternative was calling every outage a
# forgery and turning away the users who had bothered to log in. The match
# is loose on purpose: gh has worded it "no attestations found" and "no
# attestations were verified" across releases, and both mean the same here.
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    if verdict=$(gh attestation verify "$binary" --repo "$REPO" 2>&1); then
        say "build provenance verified"
    elif printf '%s\n' "$verdict" | grep -qi 'no attestations'; then
        die "Provenance verification failed: this binary does not carry an
attestation from $REPO's release workflow. Nothing was installed."
    else
        say "provenance not verified: gh attestation verify did not finish, so
this says nothing about the binary either way. It reported:
$verdict
To check by hand once gh can reach GitHub:
       gh attestation verify $INSTALL_DIR/virga --repo $REPO"
    fi
else
    say "provenance not verified (needs an authenticated gh). To check by hand:
       gh attestation verify $INSTALL_DIR/virga --repo $REPO"
fi

mkdir -p "$INSTALL_DIR"

# Stage beside the target and rename, rather than moving straight out of the
# temporary directory. A rename within one directory is atomic; `mv` across
# filesystems is a copy followed by an unlink, and on Linux $TMPDIR is usually
# tmpfs while the install directory is not. An interrupted copy would leave a
# truncated binary on PATH, which is worse than no binary at all.
staged="$INSTALL_DIR/.virga.install.$$"
cp "$binary" "$staged"
chmod +x "$staged"
mv -f "$staged" "$INSTALL_DIR/virga"

# Running what was just installed is the only check that the binary matches
# the machine. It must not be fatal, though: the install already succeeded, and
# a binary that will not start is worth a different message, not a stack trace.
if installed=$("$INSTALL_DIR/virga" --version 2>/dev/null); then
    say "installed $installed to $INSTALL_DIR/virga"
else
    say "installed virga $version to $INSTALL_DIR/virga, but it would not run here."
fi

# --- is it reachable --------------------------------------------------------

# The line to run so the next terminal finds the binary, chosen by the
# user's shell. Guessing `~/.<shell>rc` from the shell's name got two common
# cases wrong: fish neither reads `~/.fishrc` nor accepts `export`, and a
# login shell on macOS reads `~/.bash_profile`, not `~/.bashrc`, so bash
# users there were told to edit a file Terminal.app never sources. Naming
# `~/.bash_profile` has its own trap, that bash reads it *instead of*
# `~/.profile` once it exists, but a fresh macOS account has neither file
# and every common terminal there opens a login shell, so it is the right
# single answer. A shell this does not know gets the bare `export` line,
# right for any Bourne-family shell and at least harmless elsewhere.
#
# `$PATH` must reach the rc file literally; expanding it here would freeze
# today's PATH into it. shellcheck's SC2016 warns about exactly that, and
# the single quotes are the point. scripts/check-path-advice.sh pins every
# arm.
path_advice() {
    # shellcheck disable=SC2016
    export_line='export PATH="'"$INSTALL_DIR"':$PATH"'
    case "$(basename -- "${SHELL:-sh}")" in
        fish)
            printf '    fish_add_path "%s"\n' "$INSTALL_DIR" ;;
        zsh)
            printf "    echo '%s' >> ~/.zshrc\n" "$export_line" ;;
        bash)
            if [ "$(uname -s)" = Darwin ]; then
                printf "    echo '%s' >> ~/.bash_profile\n" "$export_line"
            else
                printf "    echo '%s' >> ~/.bashrc\n" "$export_line"
            fi ;;
        *)
            printf '    %s\n' "$export_line" ;;
    esac
}

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf '\n'
        say "$INSTALL_DIR is not on your PATH. Add it with:"
        printf '\n'
        path_advice
        printf '\n'
        ;;
esac
