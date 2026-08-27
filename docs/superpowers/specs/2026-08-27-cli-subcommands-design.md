# CLI Subcommands: `virga theme` and `virga update`

## Goal

Give Virga a small command-line surface for the two questions a terminal is
the wrong place to answer: "make this theme my default" and "am I running the
latest release?". Two subcommands:

- `virga theme [name]` — list the themes, or persist one as the startup
  default.
- `virga update` — check the latest release and say how to get it, matched to
  how this copy was installed.

Both answer and exit without taking over the terminal, like `--version` and
`--help` today. The TUI itself is unchanged: every in-app control stays a key,
and no flag changes how the application runs once it is running.

## Why this does not break the "no options" philosophy

The help text currently says *"Virga takes no options that change how it
runs — the terminal is the whole interface."* That remains true. These
subcommands are not options to the application; they are questions and
declarations handled before the application starts, the same category as
`--version`. The line moves from "no options" to "no options that change a
run" — the theme subcommand writes a preference the next run reads, exactly
as pressing `l` writes a location the next run reads.

## User-visible behavior

### `virga theme`

Lists every theme, one per line, marking the current startup default:

```
$ virga theme
  default       the terminal's own sixteen colours
* gruvbox dark  warm — orange bars, gold selection, green today
  nord          cool — icy bars, aurora-purple selection
  tokyo night   blue and violet, one warm selection
  dracula       loud — pink bars, lime selection, cyan today

The marked theme is the startup default. VIRGA_THEME overrides it for one
launch; t cycles themes inside the app for one session.
```

With nothing persisted, the marker sits on `default`.

### `virga theme <name>`

Persists a startup theme and confirms it:

```
$ virga theme tokyo night
virga: startup theme is now tokyo night.
```

- Names get the `VIRGA_THEME` treatment: case-insensitive, separators
  interchangeable, and multi-word names work unquoted — the arguments after
  `theme` are joined with spaces before parsing, so `virga theme tokyo night`,
  `virga theme tokyo-night`, and `virga theme Tokyo_Night` are one command.
- An unknown name is an error (exit 2) that lists the known themes. Unlike
  the environment variable — where a typo must not stop the weather — an
  explicit command asked a question and deserves a real answer, not a
  fallback.
- Persisting `default` is how you undo: it makes the built-in default
  explicit, which is indistinguishable from never having chosen.
- A failure to write the state file is an error (exit 1) naming the path,
  not a silent success.

### Startup theme precedence

From weakest to strongest:

1. built-in `default`
2. persisted theme (`virga theme <name>`)
3. `VIRGA_THEME` (one launch)
4. `t` inside the app (one session)

`VIRGA_THEME` outranks the persisted theme because an environment variable is
per-invocation and typed deliberately. An unusable `VIRGA_THEME` value warns
and falls back to the *persisted* theme, not to `default` — the standing
choice absorbs the typo.

`t` stays session-only. Cycling is for previewing, and a preview that
overwrites the default the moment you glance at it would make `t` a commitment
instead of a look around. `virga theme` is the commitment.

### `virga update`

Asks GitHub for the newest release tag, compares it with the running version,
and answers:

```
$ virga update
virga 0.2.0 is the latest release.
```

```
$ virga update
virga 0.3.0 is available (you have 0.2.0).
Installed with Homebrew — update with:

    brew upgrade virga
```

The instruction is chosen by looking at where the running binary lives:

| Binary path | Method | Instruction |
|---|---|---|
| contains `/Cellar/` or a Homebrew prefix | Homebrew | `brew upgrade virga` |
| under `~/.cargo/bin` | Cargo | `cargo install --git … --force` |
| anywhere else (the install script's territory) | Script | the `curl … \| sh` one-liner, with `VIRGA_INSTALL_DIR` shown when the binary is not in `~/.local/bin` |
| Windows | Download | the releases page URL |

- The check is one HTTPS request to `https://github.com/t-shahan/virga/releases/latest`
  with redirects disabled; the tag rides in the `Location` header. The same
  trick `install.sh` uses, for the same reasons: no JSON, no API rate limit,
  and `/releases/latest` never points at a pre-release.
- Nothing else is sent — no version string, no identifier. This is also the
  only subcommand that touches the network, and the README's privacy section
  says so.
- No answer from the network is exit 1 with a readable error, never a hang:
  the request carries the same timeout the weather client uses.
- Exit code is 0 whether or not an update exists — the command answered.
  Distinguishing "outdated" by exit code is a script-facing contract this
  command is not making; it can be added later without breaking anyone.

`virga update` does **not** replace the binary. Self-update means shipping a
tar.gz extractor, a checksum verifier, and an atomic installer inside the
program — three new dependencies and a new way to brick an install, duplicated
against `install.sh`, for the benefit of exactly one install method (Homebrew
and Cargo must not be written over behind their backs). If demand shows up, a
`virga update --install` for script installs is the follow-up, and this
command's output is already shaped for it.

### Grammar, help, and errors

- `virga help` and `virga version` join `-h/--help` and `-V/--version` as
  subcommand spellings; with subcommands existing, `virga help` is what people
  will type.
- The first argument decides everything, as today. Trailing arguments a
  subcommand does not take are usage errors (exit 2), and an unknown first
  argument stays exit 2 with the usage text — a typo must never fall through
  into the full-screen application.
- `--help` grows a `Commands:` section and keeps its promise about keys:

```
Usage: virga [COMMAND]

Commands:
  theme [NAME]   List themes, or set the startup default
  update         Check whether a newer release exists
  help, version  What -h and -V print

Options:
  -h, --help     Print this message
  -V, --version  Print the version
```

## Storage

The persisted theme lives in the existing `state.json`, as the theme's
normalised name:

```json
{
  "version": 2,
  "location": { "label": "Berlin, Germany", "lat": 52.52437, "lon": 13.41053 },
  "source": "chosen",
  "theme": "tokyo night"
}
```

One file rather than a second one: the README promises "the one file Virga
writes", and a theme name alongside a city does not strain that promise. The
privacy story is unchanged in substance and updated in wording.

Format rules, chosen so a document never claims a newer format than it needs:

- **Reading**: `theme` is optional in every version. Serde already ignores
  unknown fields, so today's binaries read a themed v2 document without
  noticing — which is what makes carrying the field in v2 safe. An unknown
  theme *name* warns and is ignored; it must not take the remembered location
  down with it.
- **Writing with a location**: version 2, with `theme` present only when set.
  Fully readable by every binary since the field was optional from the start.
- **Writing without a location**: `virga theme` before any weather has ever
  loaded has a theme and no city to save. That document is version 3, where
  `location` is optional — an old binary refuses it loudly ("unsupported
  state version 3") rather than misreading it, warns, and falls back, which
  is the contract unknown versions have always had. The moment a location is
  remembered the file returns to version 2.
- Saves stay read-merge-write through the existing atomic temp-file dance:
  saving a location preserves the theme on disk, saving a theme preserves the
  location, and a failed write leaves the previous document intact.

## Architecture

- **`src/cli.rs` (new)** owns `Invocation`, `parse_args`, and `usage`, moved
  out of `main.rs` (which is past 750 lines). Parsing stays hand-rolled: the
  grammar is five words and two flags, the error messages are the feature,
  and a dependency like clap would be the largest crate in the tree for the
  smallest job in it.
- **`src/update.rs` (new)** owns the release probe: resolving the latest tag
  from a redirect, comparing versions, classifying the install method from a
  path, and composing the instruction. Everything but the single HTTP call is
  a pure function.
- **`src/state.rs`** gains the optional-field document, a `Persisted` value
  (remembered location + theme), and merge-preserving `save_location` /
  `save_theme`. Still no networking.
- **`src/theme.rs`** is untouched except for reuse: `Theme::from_name`,
  `Theme::ALL`, and `Theme::name` already do everything the subcommand needs.
- **`main.rs`** dispatches subcommands before any terminal takeover, network
  lookup, or state write, in the same "answering must not have side effects"
  spot the version check occupies today.

## Out of scope

- Self-updating (`virga update --install`) — designed for, not built.
- Persisting the `t` key's cycling, units, or any other in-app setting.
- A general configuration file. The README's limitation shrinks ("only the
  startup theme is persisted; units last for the session") but does not
  disappear.
- Any change to which keys exist or what they do.

## Documentation

- README: `--help` excerpt, Themes section (persistence paragraph replaces
  "the theme is not written to disk"), Configuration section, Limitations,
  Updating table (add `virga update` as the way to find out), Data and
  Privacy (the update check's single request; the state file's contents).
- CHANGELOG under `Unreleased`, since the release tooling refuses to publish
  what it does not describe.
