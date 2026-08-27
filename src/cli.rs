//! What the command line asked for, decided before anything else runs.
//!
//! Parsing lives apart from `main` so the grammar and its error text can be
//! tested without a terminal, and so `main` stays the place where answers are
//! printed rather than the place where they are worked out.

/// What the command line asked for.
///
/// Virga takes no options that change how it runs — the terminal is the whole
/// interface. These exist because a binary someone installed from a tap or
/// a tarball has to be able to answer "what did I just install?" without being
/// launched into a full-screen application they then have to quit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Invocation {
    Run,
    Version,
    Help,
    /// An argument that means nothing to us. Carried rather than reported here
    /// so the caller owns the exit code and the stream it is written to.
    Unknown(String),
}

/// Classify the arguments, ignoring `argv[0]`.
///
/// Only the first argument is inspected, because none of them combine: whatever
/// it asks for is the whole answer, and `virga --help --version` printing help
/// is as good as any other rule. An unrecognized argument is an error rather
/// than something to skip past — a typo must not silently start the
/// application, because the user asked a question they would never see answered.
pub(crate) fn parse_args<I, S>(arguments: I) -> Invocation
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let Some(argument) = arguments.into_iter().next() else {
        return Invocation::Run;
    };

    match argument.as_ref() {
        "-V" | "--version" => Invocation::Version,
        "-h" | "--help" => Invocation::Help,
        other => Invocation::Unknown(other.to_string()),
    }
}

/// The `--help` text, also reused as the usage line under an argument error.
pub(crate) fn usage() -> String {
    format!(
        "\
virga {version}
{description}

Usage: virga [OPTIONS]

Options:
  -h, --help     Print this message
  -V, --version  Print the version

Virga takes no other options. Every other control is a key inside the
application, and the bar along the bottom names them. `q` quits.

Environment:
  VIRGA_THEME  Startup palette
  VIRGA_GEOIP  Set to `off` to skip the IP location lookup

Weather, air quality and geocoding come from Open-Meteo. No account or API key
is required. <{repository}>",
        version = env!("CARGO_PKG_VERSION"),
        description = env!("CARGO_PKG_DESCRIPTION"),
        repository = env!("CARGO_PKG_REPOSITORY"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_arguments_runs_the_application() {
        assert_eq!(parse_args(Vec::<String>::new()), Invocation::Run);
    }

    #[test]
    fn both_version_spellings_are_recognized() {
        assert_eq!(parse_args(["--version"]), Invocation::Version);
        assert_eq!(parse_args(["-V"]), Invocation::Version);
    }

    #[test]
    fn both_help_spellings_are_recognized() {
        assert_eq!(parse_args(["--help"]), Invocation::Help);
        assert_eq!(parse_args(["-h"]), Invocation::Help);
    }

    #[test]
    fn an_unrecognized_argument_is_carried_back_verbatim() {
        assert_eq!(
            parse_args(["--colour=blue"]),
            Invocation::Unknown("--colour=blue".to_string())
        );
    }

    #[test]
    fn a_typo_never_falls_through_to_running_the_application() {
        // The failure this guards against is a mistyped flag silently taking
        // over the terminal, where the user never sees that it was ignored.
        assert_eq!(parse_args(["-v"]), Invocation::Unknown("-v".to_string()));
        assert_eq!(
            parse_args(["--verison"]),
            Invocation::Unknown("--verison".to_string())
        );
    }

    #[test]
    fn the_first_recognized_argument_wins() {
        assert_eq!(parse_args(["--help", "--version"]), Invocation::Help);
        assert_eq!(parse_args(["--version", "--help"]), Invocation::Version);
    }

    #[test]
    fn usage_names_the_binary_the_version_and_both_environment_variables() {
        let text = usage();
        assert!(text.starts_with("virga "));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("VIRGA_THEME"));
        assert!(text.contains("VIRGA_GEOIP"));
    }
}
