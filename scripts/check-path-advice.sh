#!/bin/sh
# Pin the PATH advice install.sh prints, one arm per shell.
#
#   ./scripts/check-path-advice.sh
#
# The script as a whole cannot be run without a release to download, but
# `path_advice` is a pure function of INSTALL_DIR, SHELL and `uname -s`, so
# it is lifted out by its own braces and driven directly. The extraction is
# deliberately brittle: rename the function or move its closing brace and
# this fails loudly rather than testing nothing.
set -eu

cd "$(dirname "$0")/.."

# The range must end at the function's own brace. Were the brace indented
# away, sed would run to the end of the file and the eval would execute the
# installer itself, so the extraction is checked before anything runs.
extracted=$(sed -n '/^path_advice() {/,/^}/p' install.sh)
case "$extracted" in
    path_advice*'
}') ;;
    *) echo "could not lift path_advice out of install.sh by its braces" >&2; exit 1 ;;
esac
eval "$extracted"

# A space in the directory is the case that catches an unquoted arm. Read by
# the function `eval` brought in, which shellcheck cannot see.
# shellcheck disable=SC2034
INSTALL_DIR="/opt/my bin"

# `uname -s` is the one fact about the host that path_advice reads, so every
# check names the kernel it assumes and gets a stub that says so. Trusting the
# real one made the Linux expectation for bash fail on a Mac, where the same
# arm rightly points at ~/.bash_profile.
stub=$(mktemp -d)
trap 'rm -rf "$stub"' EXIT
for kernel in Linux Darwin; do
    mkdir "$stub/$kernel"
    printf '#!/bin/sh\necho %s\n' "$kernel" > "$stub/$kernel/uname"
    chmod +x "$stub/$kernel/uname"
done

check() {
    got=$(PATH="$stub/$1:$PATH" SHELL="$2" path_advice)
    if [ "$got" != "    $3" ]; then
        printf '%s, SHELL=%s\n  want: %s\n  got:  %s\n' "$1" "$2" "$3" "$got" >&2
        exit 1
    fi
}

# The literal `$PATH` in the expected lines is the point: it must reach the
# user's rc file unexpanded. One group, so one directive covers them all.
# shellcheck disable=SC2016
{
    check Linux  /usr/bin/fish 'fish_add_path "/opt/my bin"'
    check Linux  /bin/zsh      "echo 'export PATH=\"/opt/my bin:\$PATH\"' >> ~/.zshrc"
    check Linux  /bin/bash     "echo 'export PATH=\"/opt/my bin:\$PATH\"' >> ~/.bashrc"
    check Darwin /bin/bash     "echo 'export PATH=\"/opt/my bin:\$PATH\"' >> ~/.bash_profile"
    check Linux  /usr/bin/nu   'export PATH="/opt/my bin:$PATH"'
    check Linux  ''            'export PATH="/opt/my bin:$PATH"'
}

echo "path advice: ok"
