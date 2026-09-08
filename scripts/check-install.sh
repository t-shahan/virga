#!/bin/sh
# Drive install.sh end to end against stand-ins for everything it reaches
# out to, and pin what it does on each path.
#
#   ./scripts/check-install.sh [shell]     # the shell that runs install.sh;
#                                          # sh by default, bash worth a run
#
# The script cannot be run for real without a release to download and a
# network to download it from, and neither belongs in a test. Everything it
# talks to sits behind a shim on PATH: `curl` answers from a fixture tarball
# built here, `gh` returns the verdict the case asks for, `cp` can write half
# a file and then signal the shell, and `uname` names one platform so the
# fixture has one name. PATH is built from scratch, so the real curl and gh
# cannot be reached by accident, and the "gh is not installed" case is just
# the shim being left off.
#
# The signal cases are the ones that matter most. The whole point of the
# staged copy in install.sh is that an interrupted install leaves nothing on
# PATH, and a trap that cleans up but does not exit lets the script carry on
# past the signal and rename a truncated file. Nothing else in CI runs the
# script, so that regression would be silent.
set -eu

cd "$(dirname "$0")/.."

# Resolved before PATH is replaced: the shell under test, the real cp the
# shim hands over to, and the checksum tool install.sh will also pick.
shell=$(command -v "${1:-sh}") || { echo "no such shell: $1" >&2; exit 1; }
real_cp=$(command -v cp)
if command -v sha256sum >/dev/null 2>&1; then
    sha256() { sha256sum "$1" | cut -d' ' -f1; }
else
    sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
fi

root=$(mktemp -d)
trap 'rm -rf "$root"' EXIT

# --- the environment install.sh runs in ------------------------------------

# Only what install.sh and the shims need, linked by name, so a tool the
# script grows a dependency on shows up here as a failure rather than being
# found on the host by luck.
mkdir -p "$root/sys" "$root/bin" "$root/gh" "$root/fixture"
for tool in tar gzip find head grep cut mktemp mkdir chmod mv basename sed rm cat dd ls; do
    ln -s "$(command -v "$tool")" "$root/sys/$tool"
done
for tool in sha256sum shasum; do
    if command -v "$tool" >/dev/null 2>&1; then
        ln -s "$(command -v "$tool")" "$root/sys/$tool"
    fi
done

# The binary is a script so the post-install `--version` run has something
# to say, and the archive layout matches release.yml: one directory named
# after the archive, the binary inside it.
name=virga-0.6.1-x86_64-unknown-linux-musl
mkdir -p "$root/fixture/$name"
printf '#!/bin/sh\nprintf "virga 0.6.1\\n"\n' > "$root/fixture/$name/virga"
chmod +x "$root/fixture/$name/virga"
tar -czf "$root/fixture/$name.tar.gz" -C "$root/fixture" "$name"
printf '%s  %s\n' "$(sha256 "$root/fixture/$name.tar.gz")" "$name.tar.gz" \
    > "$root/fixture/SHA256SUMS"
# A release that exists but was built for one other platform.
printf '%s  virga-0.5.0-aarch64-apple-darwin.tar.gz\n' "$(sha256 "$root/fixture/$name.tar.gz")" \
    > "$root/fixture/SHA256SUMS.other-platform"

cat > "$root/bin/uname" <<'EOF'
#!/bin/sh
case "$1" in
    -s) echo Linux ;;
    -m) echo x86_64 ;;
    *) exit 1 ;;
esac
EOF

# Answers by URL for a GitHub with releases v0.1.0 (no assets), v0.5.0 (the
# other platform only) and v0.6.1 (the fixture, and the latest). SHIM_CURL
# picks one failure. %{http_code} is written even when -f makes the exit
# status non-zero, and reads 000 when nothing answered, which is what the
# real curl does and what install.sh's tag probe depends on.
cat > "$root/bin/curl" <<'EOF'
#!/bin/sh
printf 'curl %s\n' "$*" >> "$SHIM_LOG"
url= out= write=
while [ $# -gt 0 ]; do
    case "$1" in
        -o) out=$2; shift ;;
        -w) write=$2; shift ;;
        https://*) url=$1 ;;
    esac
    shift
done
answer() {
    [ "$write" != '%{http_code}' ] || printf '%s' "$1"
    [ "$2" -eq 0 ] || printf 'curl: (%s) shim failure for %s\n' "$2" "$url" >&2
    exit "$2"
}
case "$url" in
    */releases/latest)
        printf 'https://github.com/t-shahan/virga/releases/tag/v0.6.1' ;;
    */releases/tag/*)
        case "${SHIM_CURL:-}" in
            probe-403) answer 403 22 ;;
            probe-7) answer 000 7 ;;
            probe-28) answer 000 28 ;;
        esac
        case "${url##*/}" in
            v0.1.0|v0.5.0|v0.6.1) answer 200 0 ;;
            *) answer 404 22 ;;
        esac ;;
    */releases/download/*/SHA256SUMS)
        [ "${SHIM_CURL:-}" != sums-28 ] || answer 000 28
        tag=${url#*/download/}
        case "${tag%/SHA256SUMS}" in
            v0.6.1) cat "$SHIM_FIXTURE/SHA256SUMS" > "$out" ;;
            v0.5.0) cat "$SHIM_FIXTURE/SHA256SUMS.other-platform" > "$out" ;;
            *) answer 404 22 ;;
        esac ;;
    */releases/download/*.tar.gz)
        cat "$SHIM_FIXTURE/${url##*/}" > "$out" ;;
    *) printf 'curl shim: unexpected url %s\n' "$url" >&2; exit 99 ;;
esac
EOF

# The wording is gh's own, from cli/cli: `no attestations found` when nothing
# signed the binary, `no attestations were verified` when something did but
# the policy rejected it, and the HTTP error verbatim when the API could not
# be reached. install.sh tells them apart by text, so the text is the test.
cat > "$root/gh/gh" <<'EOF'
#!/bin/sh
printf 'gh %s\n' "$*" >> "$SHIM_LOG"
case "$1 $2" in
    'auth status') exit 0 ;;
    'attestation verify')
        case "${SHIM_GH:-verified}" in
            verified)
                echo 'Loaded digest sha256:0000 for file://virga'
                echo 'Verification succeeded!'
                exit 0 ;;
            not-found)
                echo 'Error: no attestations found' >&2
                exit 1 ;;
            not-verified)
                echo 'Error: no attestations were verified' >&2
                exit 1 ;;
            outage)
                echo 'Error: failed to fetch attestations from t-shahan/virga: HTTP 503: Service Unavailable' >&2
                exit 1 ;;
        esac ;;
esac
printf 'gh shim: unexpected %s\n' "$*" >&2
exit 99
EOF

# With SHIM_CP set, writes the first 20 bytes, delivers that signal to the
# shell that called it, and exits 0 with the file still truncated. The shell
# runs the trap once cp returns, so this is exactly the moment install.sh's
# traps are for: a copy that stopped short, and a script that has every
# chance to chmod and rename it anyway.
cat > "$root/bin/cp" <<'EOF'
#!/bin/sh
if [ -z "${SHIM_CP:-}" ]; then
    exec "$SHIM_REAL_CP" "$@"
fi
dd if="$1" of="$2" bs=20 count=1 2>/dev/null
kill -"$SHIM_CP" $PPID
exit 0
EOF
chmod +x "$root/bin/"* "$root/gh/gh"

# --- running one case ------------------------------------------------------

hardening='--proto =https --proto-redir =https --tlsv1.2 --connect-timeout 15 --max-time 300'

# run <case> [NOGH] [VAR=value ...]
#
# Each case gets its own install and temp directory so leftovers are
# attributable, and a fresh environment so one case's shim settings cannot
# leak into the next. What happened lands in $status, $out, $err and $log.
run() {
    label=$1
    shift
    dir="$root/case-$label"
    inst="$dir/inst"
    out="$dir/out"
    err="$dir/err"
    log="$dir/calls"
    mkdir -p "$inst" "$dir/tmp"
    : > "$log"
    path="$root/gh:$root/bin:$root/sys"
    if [ "${1:-}" = NOGH ]; then
        path="$root/bin:$root/sys"
        shift
    fi
    status=0
    env -i PATH="$path" HOME="$dir" TMPDIR="$dir/tmp" \
        VIRGA_INSTALL_DIR="$inst" \
        SHIM_LOG="$log" SHIM_FIXTURE="$root/fixture" SHIM_REAL_CP="$real_cp" \
        "$@" "$shell" install.sh > "$out" 2> "$err" || status=$?
}

fail() {
    printf 'check-install: %s: %s\n--- stdout\n' "$label" "$1" >&2
    cat "$out" >&2
    printf -- '--- stderr\n' >&2
    cat "$err" >&2
    exit 1
}

expect_exit() { [ "$status" -eq "$1" ] || fail "exit status $status, want $1"; }
expect_out() { grep -qF -- "$1" "$out" || fail "stdout does not say: $1"; }
expect_err() { grep -qF -- "$1" "$err" || fail "stderr does not say: $1"; }
expect_no_out() { ! grep -qF -- "$1" "$out" || fail "stdout should not say: $1"; }

# Every curl call carries the timeouts and the protocol pin. A call that
# bypassed `fetch` would be the regression this guards against.
expect_hardened() {
    grep -q '^curl ' "$log" || fail "curl was never called"
    if grep '^curl ' "$log" | grep -vF -- "$hardening" >/dev/null; then
        fail "a curl call lacks the hardening flags: $(grep '^curl ' "$log" | grep -vF -- "$hardening")"
    fi
}

expect_installed() {
    [ "$(ls -A "$inst")" = virga ] || fail "install dir holds: $(ls -A "$inst")"
    [ -x "$inst/virga" ] || fail "installed binary is not executable"
    [ -z "$(ls -A "$dir/tmp")" ] || fail "temp dir not cleaned: $(ls -A "$dir/tmp")"
}

expect_nothing_left() {
    [ -z "$(ls -A "$inst")" ] || fail "install dir holds: $(ls -A "$inst")"
    [ -z "$(ls -A "$dir/tmp")" ] || fail "temp dir not cleaned: $(ls -A "$dir/tmp")"
}

# --- the cases -------------------------------------------------------------

run happy
expect_exit 0
expect_hardened
expect_installed
expect_out 'checksum verified'
expect_out 'build provenance verified'
expect_out 'installed virga 0.6.1'

run unknown-tag VIRGA_VERSION=v9.9.9
expect_exit 1
expect_hardened
expect_nothing_left
expect_err 'There is no release tagged v9.9.9'

run tag-without-sums VIRGA_VERSION=v0.1.0
expect_exit 1
expect_hardened
expect_nothing_left
expect_err 'Release v0.1.0 has no SHA256SUMS'

run tag-without-platform VIRGA_VERSION=v0.5.0
expect_exit 1
expect_hardened
expect_nothing_left
expect_err 'Release v0.5.0 has no build for x86_64-unknown-linux-musl'

# A real tag, with the probe failing for reasons that say nothing about it.
# Each must be reported as the probe failing, never as the tag being absent.
for failure in probe-403:'github.com answered 403' probe-7:'github.com did not answer' probe-28:'github.com did not answer'; do
    run "${failure%%:*}" VIRGA_VERSION=v0.1.0 SHIM_CURL="${failure%%:*}"
    expect_exit 1
    expect_nothing_left
    expect_err "could not check whether v0.1.0 exists: ${failure#*:}"
    ! grep -q 'no release tagged' "$err" || fail "a failed probe was reported as a missing tag"
done

run sums-28 SHIM_CURL=sums-28
expect_exit 1
expect_nothing_left
expect_err 'Could not download https://github.com/t-shahan/virga/releases/download/v0.6.1/SHA256SUMS'
! grep -q 'no release tagged' "$err" || fail "a timeout was reported as a missing tag"

for verdict in not-found not-verified; do
    run "gh-$verdict" SHIM_GH="$verdict"
    expect_exit 1
    expect_nothing_left
    expect_err 'Provenance verification failed'
done

run gh-outage SHIM_GH=outage
expect_exit 0
expect_installed
expect_out 'provenance not verified: gh attestation verify did not finish'
expect_out 'HTTP 503'
expect_out 'gh attestation verify'

run gh-missing NOGH
expect_exit 0
expect_installed
expect_out 'provenance not verified (needs an authenticated gh)'

for signal in INT:130 TERM:143; do
    run "${signal%%:*}-during-copy" SHIM_CP="${signal%%:*}"
    expect_exit "${signal#*:}"
    expect_nothing_left
    expect_no_out 'installed'
done

echo "install: ok"
