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
    /// List the themes, or — with a name — persist one as the startup
    /// default. The name is everything after `theme` joined with spaces, so
    /// multi-word names need no quoting.
    Theme(Option<String>),
    /// Check whether a newer release exists and say how to get it.
    Update,
    /// A recognized command given arguments it does not take. Strict where
    /// `--help --version` is lenient, because here the extra words could
    /// carry an intention — `update --install` asks for something this
    /// command will not do, and silently checking instead would be a lie.
    Usage(String),
    /// An argument that means nothing to us. Carried rather than reported here
    /// so the caller owns the exit code and the stream it is written to.
    Unknown(String),
}

/// Classify the arguments, ignoring `argv[0]`.
///
/// The first argument decides everything. Only `theme` reads past it — the
/// rest of the line is its argument — because nothing else combines: whatever
/// the first argument asks for is the whole answer, and `virga --help
/// --version` printing help is as good as any other rule. An unrecognized
/// argument is an error rather than something to skip past — a typo must not
/// silently start the application, because the user asked a question they
/// would never see answered.
pub(crate) fn parse_args<I, S>(arguments: I) -> Invocation
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut arguments = arguments.into_iter();
    let Some(argument) = arguments.next() else {
        return Invocation::Run;
    };

    match argument.as_ref() {
        "-V" | "--version" | "version" => Invocation::Version,
        "-h" | "--help" | "help" => Invocation::Help,
        "theme" => {
            let name = arguments
                .map(|argument| argument.as_ref().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            Invocation::Theme((!name.is_empty()).then_some(name))
        }
        "update" => match arguments.next() {
            None => Invocation::Update,
            Some(extra) => Invocation::Usage(format!(
                "update takes no arguments, and {:?} is one",
                extra.as_ref()
            )),
        },
        other => Invocation::Unknown(other.to_string()),
    }
}

/// The `--help` text, also reused as the usage line under an argument error.
pub(crate) fn usage() -> String {
    format!(
        "\
virga {version}
{description}

Usage: virga [COMMAND]

Commands:
  theme [NAME]   List the themes, or set the startup default
  update         Check whether a newer release exists
  help, version  What -h and -V print

Options:
  -h, --help     Print this message
  -V, --version  Print the version

No option changes how the application runs. Every control inside it is a key,
and the bar along the bottom names them. `q` quits.

Environment:
  VIRGA_THEME  Startup palette, for one launch
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
    fn help_and_version_work_as_words_too() {
        // With subcommands in the grammar, `virga help` is what people will
        // type; making it an error while `theme` works would be a trap.
        assert_eq!(parse_args(["help"]), Invocation::Help);
        assert_eq!(parse_args(["version"]), Invocation::Version);
    }

    #[test]
    fn theme_alone_asks_for_the_list() {
        assert_eq!(parse_args(["theme"]), Invocation::Theme(None));
    }

    #[test]
    fn theme_joins_its_arguments_so_multiword_names_need_no_quotes() {
        assert_eq!(
            parse_args(["theme", "tokyo", "night"]),
            Invocation::Theme(Some("tokyo night".to_string()))
        );
        assert_eq!(
            parse_args(["theme", "tokyo-night"]),
            Invocation::Theme(Some("tokyo-night".to_string()))
        );
    }

    #[test]
    fn update_takes_no_arguments() {
        assert_eq!(parse_args(["update"]), Invocation::Update);
        // The extra word could carry an intention — an install this command
        // will not perform — so it is an error, not something to skip past.
        let Invocation::Usage(complaint) = parse_args(["update", "--install"]) else {
            panic!("extra arguments after update were not a usage error");
        };
        assert!(complaint.contains("--install"));
    }

    #[test]
    fn a_subcommand_typo_is_still_unknown() {
        assert_eq!(
            parse_args(["them"]),
            Invocation::Unknown("them".to_string())
        );
        assert_eq!(
            parse_args(["themes"]),
            Invocation::Unknown("themes".to_string())
        );
    }

    #[test]
    fn usage_names_the_commands_and_both_environment_variables() {
        let text = usage();
        assert!(text.starts_with("virga "));
        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains("theme"));
        assert!(text.contains("update"));
        assert!(text.contains("VIRGA_THEME"));
        assert!(text.contains("VIRGA_GEOIP"));
    }
}
