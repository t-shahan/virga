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

# --- which release ----------------------------------------------------------

latest_tag() {
    # The redirect on /releases/latest carries the tag, which avoids both the
    # JSON parsing and the lower rate limit of the API.
    curl -fsSLI -o /dev/null -w '%{url_effective}' \
        "https://github.com/$REPO/releases/latest" \
        | sed 's#.*/tag/##'
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
# Runs on success and on failure, so a network error leaves nothing behind.
trap 'rm -rf "$work"' EXIT INT TERM

say "downloading $archive"
curl -fsSL "$base/$archive" -o "$work/$archive" \
    || die "No such archive: $base/$archive"
curl -fsSL "$base/SHA256SUMS" -o "$work/SHA256SUMS" \
    || die "Release $tag has no SHA256SUMS, so the download cannot be verified."

expected=$(grep " ${archive}\$" "$work/SHA256SUMS" | cut -d' ' -f1)
[ -n "$expected" ] || die "SHA256SUMS does not list $archive."

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
if command -v gh >/dev/null 2>&1 && gh auth status >/dev/null 2>&1; then
    if gh attestation verify "$binary" --repo "$REPO" >/dev/null 2>&1; then
        say "build provenance verified"
    else
        die "Provenance verification failed: this binary does not carry an
attestation from $REPO's release workflow. Nothing was installed."
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
trap 'rm -rf "$work"; rm -f "$staged"' EXIT INT TERM
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

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        printf '\n'
        say "$INSTALL_DIR is not on your PATH. Add it with:"
        # $PATH must reach the user's rc file literally; expanding it here
        # would freeze today's PATH into it.
        # shellcheck disable=SC2016
        printf '\n    echo '"'"'export PATH="%s:$PATH"'"'"' >> ~/.%src\n\n' \
            "$INSTALL_DIR" "$(basename "${SHELL:-sh}")"
        ;;
esac
