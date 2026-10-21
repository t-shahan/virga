//! Is there a newer release, and what should this particular install do
//! about it?
//!
//! One request answers the first question: GitHub redirects
//! `releases/latest` to the newest release's tag page, so the tag rides the
//! `Location` header of a response this module never follows — the same
//! trick `install.sh` uses, for the same reasons. No JSON, no API rate
//! limit, and `releases/latest` never points at a pre-release. Everything
//! else here is a pure function, so the answer's wording is testable without
//! a network.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use ureq::Agent;

/// The release listing the probe asks, `/latest` appended.
pub(crate) const RELEASES_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/releases");

/// A user asked and is waiting at a prompt, so the budget is the tight one:
/// the detection budget, not the forecast's fifteen seconds.
const TIMEOUT_GLOBAL: Duration = Duration::from_secs(5);
const TIMEOUT_CONNECT: Duration = Duration::from_secs(3);

/// Redirects off is the whole request: the `Location` header of the redirect
/// *is* the answer, and following it would download a release page nobody
/// asked for. With `max_redirects(0)` ureq hands back the 3xx as a response.
fn probe_agent() -> Agent {
    Agent::new_with_config(
        Agent::config_builder()
            .timeout_global(Some(TIMEOUT_GLOBAL))
            .timeout_connect(Some(TIMEOUT_CONNECT))
            .max_redirects(0)
            .build(),
    )
}

/// The newest release's tag, e.g. `v0.2.0`.
pub(crate) fn latest_tag(releases_url: &str) -> Result<String> {
    latest_tag_with(&probe_agent(), releases_url)
}

fn latest_tag_with(agent: &Agent, releases_url: &str) -> Result<String> {
    let response = agent
        .get(format!("{releases_url}/latest"))
        .call()
        .context("ask github for the latest release")?;
    let location = response
        .headers()
        .get("location")
        .context("the release lookup did not redirect")?
        .to_str()
        .context("the redirect location is not text")?;
    let (_, tag) = location
        .rsplit_once("/tag/")
        .with_context(|| format!("the redirect went somewhere unexpected: {location}"))?;
    anyhow::ensure!(!tag.is_empty(), "the redirect named no tag");
    Ok(tag.to_string())
}

/// A release version: the `x.y.z` triple, and whatever pre-release marker —
/// the `rc1` of `v0.2.0-rc1` — hangs off it.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct Release {
    major: u64,
    minor: u64,
    patch: u64,
    /// Compared only as "present is older than absent": an `rc1` of `0.2.0`
    /// is behind `0.2.0`, and ordering two pre-releases against each other is
    /// a judgement this module never needs to make — `releases/latest` never
    /// points at one.
    prerelease: Option<String>,
}

impl Release {
    /// Parse a tag or version, with or without the leading `v`. Anything that
    /// is not `x.y.z` with an optional `-suffix` is an error, not a guess: a
    /// wrong comparison would tell someone to "update" to what they have.
    pub(crate) fn parse(tag: &str) -> Result<Release> {
        let version = tag.trim();
        let version = version.strip_prefix('v').unwrap_or(version);
        let (triple, prerelease) = match version.split_once('-') {
            Some((triple, prerelease)) if !prerelease.is_empty() => {
                // The marker is echoed to a terminal and arrives off the
                // network, so it is held to what semver allows. Anything
                // else — a control character riding a hostile redirect —
                // is not a version, and must never reach stdout.
                anyhow::ensure!(
                    prerelease
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-'),
                    "{tag:?} has a pre-release marker with characters outside 0-9A-Za-z.-"
                );
                (triple, Some(prerelease.to_string()))
            }
            Some(_) => anyhow::bail!("{tag:?} has an empty pre-release marker"),
            None => (version, None),
        };

        let numbers: Vec<u64> = triple
            .split('.')
            .map(|part| {
                part.parse::<u64>()
                    .with_context(|| format!("{tag:?} is not a version"))
            })
            .collect::<Result<_>>()?;
        let [major, minor, patch] = numbers[..] else {
            anyhow::bail!("{tag:?} is not a version");
        };
        Ok(Release {
            major,
            minor,
            patch,
            prerelease,
        })
    }

    /// Whether `self` is an update for someone running `current`.
    pub(crate) fn newer_than(&self, current: &Release) -> bool {
        let this = (self.major, self.minor, self.patch);
        let that = (current.major, current.minor, current.patch);
        // A pre-release sits behind its own bare triple: someone on 0.2.0-rc1
        // is behind 0.2.0.
        this > that || (this == that && self.prerelease.is_none() && current.prerelease.is_some())
    }
}

impl std::fmt::Display for Release {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(prerelease) = &self.prerelease {
            write!(f, "-{prerelease}")?;
        }
        Ok(())
    }
}

/// How this copy got here, judged from where the binary lives. The method
/// decides the instruction: Homebrew and Cargo own their installs and must
/// not be written over behind their backs, and everywhere else is the
/// install script's territory.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InstallMethod {
    Homebrew,
    Cargo,
    /// Carries the directory to name when it is not the script's default,
    /// so the one-liner puts the new binary where the old one is.
    Script {
        install_dir: Option<PathBuf>,
    },
    /// Windows, which the install script does not cover.
    Download,
}

/// Where the running binary really lives.
///
/// `current_exe` answers with the path the binary was invoked through, and
/// on macOS that is not symlink-resolved. An Intel Homebrew install runs as
/// `/usr/local/bin/virga`, a link into the Cellar; judged by the link it
/// looked like the script's work, and the advice was to pipe `install.sh`
/// over Homebrew's symlink, which is the one outcome this module exists to
/// prevent. A path that cannot be resolved is judged as it was invoked.
pub(crate) fn running_binary() -> Option<PathBuf> {
    std::env::current_exe().ok().map(resolve_links)
}

fn resolve_links(exe: PathBuf) -> PathBuf {
    std::fs::canonicalize(&exe).unwrap_or(exe)
}

/// Classify the running binary's path. `windows` is passed rather than read
/// from `cfg!` so the branch is testable everywhere; the caller passes
/// `cfg!(windows)`.
pub(crate) fn install_method(
    exe: Option<&Path>,
    home: Option<&Path>,
    windows: bool,
) -> InstallMethod {
    if windows {
        return InstallMethod::Download;
    }
    let Some(exe) = exe else {
        // Nowhere to look means no better guess than the script's default.
        return InstallMethod::Script { install_dir: None };
    };

    // Whole components, not substrings: a checkout of the tap lives in a
    // directory called homebrew-virga, and a username can contain either
    // word. With the path resolved above, every Homebrew binary passes
    // through a Cellar; the prefixes cover the unresolved fallback.
    let under_homebrew = exe.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some("Cellar" | "homebrew" | ".linuxbrew")
        )
    });
    if under_homebrew {
        return InstallMethod::Homebrew;
    }

    // The binary arrives resolved, so the directories it is measured against
    // have to be resolved the same way, or a `~/.cargo` that is itself a
    // symlink puts its binary somewhere the unresolved path never names and
    // a Cargo install is told to run the script over itself. A directory
    // that does not exist resolves to itself, which is still the right
    // comparison for an exe that could not be resolved either.
    let cargo_dir = home.map(|home| resolve_links(home.join(".cargo").join("bin")));
    if let (Some(parent), Some(cargo_dir)) = (exe.parent(), cargo_dir.as_deref())
        && parent == cargo_dir
    {
        return InstallMethod::Cargo;
    }

    let default_dir = home.map(|home| resolve_links(home.join(".local").join("bin")));
    let install_dir = exe
        .parent()
        .filter(|parent| Some(*parent) != default_dir.as_deref())
        .map(Path::to_path_buf);
    InstallMethod::Script { install_dir }
}

/// The whole of `virga update`'s stdout, given what the probe found.
pub(crate) fn report(current: &Release, latest: &Release, method: &InstallMethod) -> String {
    if !latest.newer_than(current) {
        return if current == latest {
            format!("virga {current} is the latest release.")
        } else {
            // A pre-release or a build ahead of the listing. Saying "latest"
            // would be untrue in both directions.
            format!("virga {current} is not behind the latest release ({latest}).")
        };
    }

    format!(
        "virga {latest} is available (you have {current}).\n{}",
        instruction(method)
    )
}

/// What `virga update` found, which is what its exit status reports.
///
/// The text on stdout already says all of this; the status repeats it so a
/// prompt or a cron job can act on the answer without scraping it, the way
/// `brew outdated` is non-zero when there is something to do.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Outcome {
    /// A newer release exists.
    Available,
    /// Nothing to do: on the latest release, or ahead of it.
    Current,
    /// The probe did not answer, so nothing is known either way.
    Failed,
}

/// Whether there is anything to do, judged the same way `report` judges it.
pub(crate) fn outcome(current: &Release, latest: &Release) -> Outcome {
    if latest.newer_than(current) {
        Outcome::Available
    } else {
        Outcome::Current
    }
}

/// The exit status `virga update` ends with.
///
/// Three, not one, for a newer release: 1 already means the check could
/// not be made and 2 is a usage error everywhere in the binary, so a script
/// that treats non-zero as failure keeps its meaning and one that wants to
/// know can tell the codes apart.
pub(crate) fn exit_code(outcome: Outcome) -> i32 {
    match outcome {
        Outcome::Current => 0,
        Outcome::Failed => 1,
        Outcome::Available => 3,
    }
}

/// The one-line startup notice, or `None` when there is nothing newsworthy.
///
/// It points at `virga update` rather than carrying the instruction itself:
/// one muted line has room to say that news exists, and the subcommand is
/// where the full answer already lives.
pub(crate) fn notice(current: &Release, latest: &Release) -> Option<String> {
    latest.newer_than(current).then(|| {
        format!("update: virga {latest} is available (you have {current}) — run `virga update`")
    })
}

fn instruction(method: &InstallMethod) -> String {
    let repository = env!("CARGO_PKG_REPOSITORY");
    match method {
        InstallMethod::Homebrew => {
            "Installed with Homebrew — update with:\n\n    brew upgrade virga".to_string()
        }
        InstallMethod::Cargo => format!(
            "Installed with Cargo — update with:\n\n    cargo install --git {repository} --force"
        ),
        InstallMethod::Download => {
            format!("Download the new release from:\n\n    {repository}/releases/latest")
        }
        InstallMethod::Script { install_dir } => {
            let script =
                "curl -fsSL https://raw.githubusercontent.com/t-shahan/virga/main/install.sh";
            // The variable rides the `sh` side of the pipe: a prefix on
            // `curl` would never reach the script it feeds.
            let run = match install_dir {
                Some(directory) => {
                    format!(
                        "{script} | VIRGA_INSTALL_DIR={} sh",
                        shell_quoted(directory)
                    )
                }
                None => format!("{script} | sh"),
            };
            format!("Update with the install script, which overwrites in place:\n\n    {run}")
        }
    }
}

/// POSIX-quote a path for the one command line the answer asks someone to
/// paste: a space must not split the assignment, and a metacharacter — a
/// `$(...)` in a directory name — must not execute when it is run. Single
/// quotes disarm everything but themselves, and an embedded quote becomes
/// the standard `'\''`.
fn shell_quoted(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    /// A loopback server answering every request with one canned response —
    /// the `weather::client` pattern, minus the shared body plumbing this
    /// module does not need.
    fn serving(status_line: &str, extra_headers: &str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let response = format!(
            "HTTP/1.1 {status_line}\r\n{extra_headers}Content-Length: 0\r\nConnection: close\r\n\r\n"
        );

        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut scratch = [0u8; 4096];
                let _ = stream.read(&mut scratch);
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
            }
        });

        format!("http://{addr}/releases")
    }

    fn test_agent() -> Agent {
        Agent::new_with_config(
            Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(5)))
                .timeout_connect(Some(Duration::from_secs(2)))
                .max_redirects(0)
                .build(),
        )
    }

    #[test]
    fn the_tag_rides_the_redirect() {
        let base = serving(
            "302 Found",
            "Location: https://github.com/t-shahan/virga/releases/tag/v0.3.0\r\n",
        );

        let tag = latest_tag_with(&test_agent(), &base).unwrap();
        assert_eq!(tag, "v0.3.0");

        // The tag off the wire, carried through to the status a script sees.
        let latest = Release::parse(&tag).unwrap();
        let current = Release::parse("0.2.0").unwrap();
        assert_eq!(exit_code(outcome(&current, &latest)), 3);
    }

    /// A page instead of a redirect — GitHub down in some novel way, or a
    /// captive portal's cheerful 200 — must fail, not read as a tag.
    #[test]
    fn an_answer_that_does_not_redirect_is_an_error() {
        let base = serving("200 OK", "");
        assert!(latest_tag_with(&test_agent(), &base).is_err());
    }

    #[test]
    fn a_redirect_going_somewhere_unexpected_is_an_error() {
        let base = serving("302 Found", "Location: https://github.com/login\r\n");
        assert!(latest_tag_with(&test_agent(), &base).is_err());
    }

    #[test]
    fn nothing_listening_is_an_error_not_a_hang() {
        // Bind then drop, so the port is almost certainly free and unserved.
        let addr = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            listener.local_addr().expect("local addr")
        };

        assert!(latest_tag_with(&test_agent(), &format!("http://{addr}/releases")).is_err());
    }

    /// The probe answers a user waiting at a prompt, so its budget has to be
    /// the tight one — actually carried by the agent, not declared nearby.
    #[test]
    fn the_probe_agent_is_bounded_and_does_not_follow() {
        let agent = probe_agent();
        let config = agent.config();

        assert_eq!(config.timeouts().global, Some(TIMEOUT_GLOBAL));
        assert_eq!(config.timeouts().connect, Some(TIMEOUT_CONNECT));
        assert!(TIMEOUT_CONNECT < TIMEOUT_GLOBAL);
        assert_eq!(config.max_redirects(), 0, "the redirect is the answer");
    }

    #[test]
    fn tags_parse_with_and_without_the_leading_v() {
        assert_eq!(
            Release::parse("v0.2.0").unwrap(),
            Release::parse("0.2.0").unwrap()
        );
    }

    #[test]
    fn a_tag_that_is_not_a_version_is_an_error_not_a_guess() {
        for tag in [
            "", "v", "0.2", "0.2.0.1", "latest", "v0.2.x", "0.2.0-", "1.2.-3",
        ] {
            assert!(Release::parse(tag).is_err(), "{tag:?} was accepted");
        }
    }

    /// The tag rides GitHub's redirect and its pre-release marker is echoed
    /// to the terminal, so a marker outside semver's alphabet — above all a
    /// control character — must be an error, never something to print.
    #[test]
    fn a_pre_release_marker_outside_semver_is_rejected() {
        for tag in [
            "0.2.0-rc\u{1b}]0;owned\u{7}",
            "0.2.0-rc 1",
            "0.2.0-rc\n1",
            "0.2.0-rc_1",
        ] {
            assert!(Release::parse(tag).is_err(), "{tag:?} was accepted");
        }
        assert!(Release::parse("0.2.0-rc.1").is_ok(), "semver dots are fine");
    }

    #[test]
    fn comparison_is_numeric_not_lexicographic() {
        let newer = Release::parse("0.10.0").unwrap();
        let older = Release::parse("0.9.0").unwrap();

        assert!(newer.newer_than(&older));
        assert!(!older.newer_than(&newer));
    }

    /// The repo has shipped v0.2.0-rc1; someone running it must be told
    /// 0.2.0 is an update.
    #[test]
    fn an_rc_of_a_version_is_older_than_its_release() {
        let release = Release::parse("0.2.0").unwrap();
        let rc = Release::parse("0.2.0-rc1").unwrap();

        assert!(release.newer_than(&rc));
        assert!(!rc.newer_than(&release));
    }

    #[test]
    fn a_release_is_not_newer_than_itself() {
        let release = Release::parse("0.2.0").unwrap();
        assert!(!release.newer_than(&release));
    }

    #[test]
    fn a_release_round_trips_through_display() {
        for version in ["0.2.0", "0.2.0-rc1", "1.10.3"] {
            assert_eq!(Release::parse(version).unwrap().to_string(), version);
        }
    }

    fn home() -> PathBuf {
        PathBuf::from("/home/someone")
    }

    #[test]
    fn a_cellar_or_homebrew_path_means_brew() {
        for exe in [
            "/opt/homebrew/Cellar/virga/0.2.0/bin/virga",
            "/usr/local/Cellar/virga/0.2.0/bin/virga",
            "/home/linuxbrew/.linuxbrew/bin/virga",
        ] {
            assert_eq!(
                install_method(Some(Path::new(exe)), Some(&home()), false),
                InstallMethod::Homebrew,
                "{exe}"
            );
        }
    }

    /// The Intel-Mac case: Homebrew's `bin` is a directory of symlinks into
    /// the Cellar, and `current_exe` hands back the link, not its target.
    /// Unresolved, nothing in `/usr/local/bin/virga` says Homebrew and the
    /// user was told to overwrite the link with `install.sh`.
    #[cfg(unix)]
    #[test]
    fn a_symlink_into_the_cellar_means_brew() {
        let prefix = tempfile::tempdir().unwrap();
        let cellar = prefix.path().join("Cellar/virga/0.5.3/bin");
        std::fs::create_dir_all(&cellar).unwrap();
        std::fs::write(cellar.join("virga"), b"").unwrap();
        let bin = prefix.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let link = bin.join("virga");
        std::os::unix::fs::symlink(cellar.join("virga"), &link).unwrap();

        let resolved = resolve_links(link.clone());
        assert_ne!(resolved, link, "the link was not followed");
        assert_eq!(
            install_method(Some(&resolved), Some(&home()), false),
            InstallMethod::Homebrew
        );
    }

    /// `~/.cargo` moved onto another disk and left behind as a symlink. The
    /// binary resolves to where the directory really is, so the directory it
    /// is compared against has to be resolved too, or the install is judged
    /// the script's and the advice is to overwrite a Cargo binary.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_cargo_directory_still_means_cargo() {
        let root = tempfile::tempdir().unwrap();
        let tools_bin = root.path().join("tools/bin");
        std::fs::create_dir_all(&tools_bin).unwrap();
        std::fs::write(tools_bin.join("virga"), b"").unwrap();
        let home = root.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink(root.path().join("tools"), home.join(".cargo")).unwrap();

        let invoked = home.join(".cargo/bin/virga");
        let resolved = resolve_links(invoked.clone());
        assert_ne!(resolved, invoked, "the link was not followed");
        assert_eq!(
            install_method(Some(&resolved), Some(&home), false),
            InstallMethod::Cargo
        );
    }

    /// The same for the script's default directory: reached through a link,
    /// it is still the default, and the one-liner needs no directory named.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_default_directory_needs_no_naming() {
        let root = tempfile::tempdir().unwrap();
        let real_bin = root.path().join("elsewhere/bin");
        std::fs::create_dir_all(&real_bin).unwrap();
        std::fs::write(real_bin.join("virga"), b"").unwrap();
        let home = root.path().join("home");
        std::fs::create_dir_all(home.join(".local")).unwrap();
        std::os::unix::fs::symlink(&real_bin, home.join(".local/bin")).unwrap();

        let resolved = resolve_links(home.join(".local/bin/virga"));
        assert_eq!(
            install_method(Some(&resolved), Some(&home), false),
            InstallMethod::Script { install_dir: None }
        );
    }

    /// A path that does not exist resolves to itself rather than to nothing,
    /// so the classification still has something to judge.
    #[test]
    fn an_unresolvable_path_is_judged_as_invoked() {
        let exe = PathBuf::from("/nowhere/at/all/virga");
        assert_eq!(resolve_links(exe.clone()), exe);
    }

    /// The words are matched as whole path components. A tap checkout or a
    /// username containing them is not a Homebrew install.
    #[test]
    fn a_homebrew_substring_is_not_a_homebrew_install() {
        for exe in [
            "/home/someone/src/homebrew-virga/target/release/virga",
            "/home/homebrewer/.local/bin/virga",
            "/home/linuxbrewfan/bin/virga",
        ] {
            assert_ne!(
                install_method(Some(Path::new(exe)), Some(&home()), false),
                InstallMethod::Homebrew,
                "{exe}"
            );
        }
        assert_eq!(
            install_method(
                Some(Path::new("/opt/homebrew/bin/virga")),
                Some(&home()),
                false
            ),
            InstallMethod::Homebrew,
            "the unresolved Apple silicon link still names its prefix"
        );
    }

    #[test]
    fn the_cargo_bin_directory_means_cargo() {
        assert_eq!(
            install_method(
                Some(Path::new("/home/someone/.cargo/bin/virga")),
                Some(&home()),
                false
            ),
            InstallMethod::Cargo
        );
    }

    /// The script's default directory needs no naming; anywhere else the
    /// one-liner has to be told where the old binary is, or it would install
    /// a second copy beside the stale one.
    #[test]
    fn anywhere_else_is_the_install_script() {
        assert_eq!(
            install_method(
                Some(Path::new("/home/someone/.local/bin/virga")),
                Some(&home()),
                false
            ),
            InstallMethod::Script { install_dir: None }
        );
        assert_eq!(
            install_method(
                Some(Path::new("/usr/local/bin/virga")),
                Some(&home()),
                false
            ),
            InstallMethod::Script {
                install_dir: Some(PathBuf::from("/usr/local/bin"))
            }
        );
    }

    #[test]
    fn windows_is_pointed_at_the_releases_page() {
        assert_eq!(
            install_method(Some(Path::new("C:\\Users\\someone\\virga.exe")), None, true),
            InstallMethod::Download
        );
    }

    #[test]
    fn an_up_to_date_binary_is_told_so_in_one_line() {
        let current = Release::parse("0.2.0").unwrap();

        let report = report(&current, &current, &InstallMethod::Homebrew);

        assert_eq!(report, "virga 0.2.0 is the latest release.");
    }

    #[test]
    fn a_build_ahead_of_the_listing_is_not_told_to_update() {
        let current = Release::parse("0.3.0").unwrap();
        let latest = Release::parse("0.2.0").unwrap();

        let report = report(&current, &latest, &InstallMethod::Homebrew);

        assert!(report.contains("0.3.0"));
        assert!(!report.contains("brew upgrade"), "there is nothing to do");
    }

    #[test]
    fn an_available_update_names_both_versions_and_one_instruction() {
        let current = Release::parse("0.2.0").unwrap();
        let latest = Release::parse("0.3.0").unwrap();

        for (method, expected) in [
            (InstallMethod::Homebrew, "brew upgrade virga"),
            (InstallMethod::Cargo, "cargo install --git"),
            (
                InstallMethod::Script { install_dir: None },
                "install.sh | sh",
            ),
            (InstallMethod::Download, "/releases/latest"),
        ] {
            let report = report(&current, &latest, &method);

            assert!(report.contains("0.3.0 is available"), "{method:?}");
            assert!(report.contains("you have 0.2.0"), "{method:?}");
            assert!(report.contains(expected), "{method:?}: {report}");
        }
    }

    /// Every code the command can end with, beside what earns it. 1 and 2
    /// are what the rest of the binary means by them; 3 is the only one
    /// `update` adds.
    #[test]
    fn each_outcome_maps_to_its_own_exit_code() {
        for (outcome, code) in [
            (Outcome::Current, 0),
            (Outcome::Failed, 1),
            (Outcome::Available, 3),
        ] {
            assert_eq!(exit_code(outcome), code, "{outcome:?}");
        }
    }

    /// The status agrees with the report: whenever stdout says an update
    /// is available the outcome is `Available`, and only then. Being ahead
    /// of the listing is not an update by either measure.
    #[test]
    fn the_outcome_tracks_the_report() {
        let current = Release::parse("0.2.0").unwrap();
        let newer = Release::parse("0.3.0").unwrap();
        let older = Release::parse("0.1.0").unwrap();

        for (latest, expected) in [
            (&newer, Outcome::Available),
            (&current, Outcome::Current),
            (&older, Outcome::Current),
        ] {
            let outcome = outcome(&current, latest);
            let report = report(&current, latest, &InstallMethod::Homebrew);

            assert_eq!(outcome, expected, "{latest}");
            assert_eq!(
                report.contains("is available"),
                outcome == Outcome::Available,
                "{latest}: {report}"
            );
        }
    }

    #[test]
    fn no_newer_release_means_no_notice() {
        let current = Release::parse("0.2.0").unwrap();
        let behind = Release::parse("0.1.0").unwrap();

        assert_eq!(notice(&current, &current), None);
        assert_eq!(notice(&current, &behind), None, "being ahead is not news");
    }

    #[test]
    fn the_notice_names_both_versions_and_points_at_the_subcommand() {
        let current = Release::parse("0.2.0").unwrap();
        let latest = Release::parse("0.3.0").unwrap();

        let notice = notice(&current, &latest).unwrap();

        assert!(notice.contains("0.3.0"));
        assert!(notice.contains("0.2.0"));
        assert!(notice.contains("virga update"));
        assert!(
            !notice.contains('\n'),
            "the notice is one line above the key bar, not a paragraph"
        );
    }

    /// The variable has to ride the `sh` side of the pipe: prefixed onto
    /// `curl` it would never reach the script it feeds.
    #[test]
    fn a_nonstandard_script_install_names_its_directory_to_sh() {
        let current = Release::parse("0.2.0").unwrap();
        let latest = Release::parse("0.3.0").unwrap();
        let method = InstallMethod::Script {
            install_dir: Some(PathBuf::from("/usr/local/bin")),
        };

        let report = report(&current, &latest, &method);

        assert!(report.contains("| VIRGA_INSTALL_DIR='/usr/local/bin' sh"));
    }

    /// The path rides a command people paste into a shell, so it is quoted
    /// against the shell: a space must not split the assignment, and a
    /// metacharacter in a directory name must not execute.
    #[test]
    fn install_directories_are_quoted_against_the_shell() {
        let current = Release::parse("0.2.0").unwrap();
        let latest = Release::parse("0.3.0").unwrap();

        for (directory, expected) in [
            (
                "/Users/some one/bin",
                "VIRGA_INSTALL_DIR='/Users/some one/bin' sh",
            ),
            (
                "/tmp/$(reboot)/bin",
                "VIRGA_INSTALL_DIR='/tmp/$(reboot)/bin' sh",
            ),
            (
                "/tmp/`reboot`/bin",
                "VIRGA_INSTALL_DIR='/tmp/`reboot`/bin' sh",
            ),
            (
                "/tmp/it's here/bin",
                r"VIRGA_INSTALL_DIR='/tmp/it'\''s here/bin' sh",
            ),
        ] {
            let method = InstallMethod::Script {
                install_dir: Some(PathBuf::from(directory)),
            };

            let report = report(&current, &latest, &method);

            assert!(report.contains(expected), "{directory}: {report}");
        }
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// The probe's one external dependency is GitHub's redirect behavior,
    /// which nothing in CI exercises. An operational smoke test, not a
    /// contract — it prints what it resolved so a human can judge it.
    #[test]
    #[ignore]
    fn real_latest_tag_resolves_and_parses() {
        let tag = latest_tag(RELEASES_URL).expect("resolve the latest tag");
        println!("latest tag: {tag}");

        Release::parse(&tag).expect("the tag parses as a version");
    }
}
