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

    let under_homebrew = exe.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "Cellar" || name.contains("homebrew") || name.contains("linuxbrew")
    });
    if under_homebrew {
        return InstallMethod::Homebrew;
    }

    if let Some(home) = home
        && exe.parent() == Some(home.join(".cargo").join("bin").as_path())
    {
        return InstallMethod::Cargo;
    }

    let default_dir = home.map(|home| home.join(".local").join("bin"));
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
                    format!("{script} | VIRGA_INSTALL_DIR={} sh", directory.display())
                }
                None => format!("{script} | sh"),
            };
            format!("Update with the install script, which overwrites in place:\n\n    {run}")
        }
    }
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

        assert_eq!(latest_tag_with(&test_agent(), &base).unwrap(), "v0.3.0");
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

        assert!(report.contains("| VIRGA_INSTALL_DIR=/usr/local/bin sh"));
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
