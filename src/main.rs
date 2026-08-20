use crate::app::{ActiveLocation, App, Fetch, LocationSource, Remembered, Screen, Startup};
use crate::events::{Message, Request};
use crate::theme::Theme;
use anyhow::{Result, anyhow};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};

mod app;
mod events;
mod input;
mod state;
mod theme;
mod ui;
mod units;
mod weather;

fn main() -> Result<()> {
    // Read before the terminal is taken over: a complaint about the variable
    // has to go to the ordinary screen, or it is written to the alternate
    // screen and wiped the moment the app exits.
    let theme = startup_theme(std::env::var("VIRGA_THEME").ok().as_deref());
    let (detect, warning) = detection_enabled(std::env::var("VIRGA_GEOIP").ok().as_deref());
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }

    let (startup, state_path) = match state::path() {
        Ok(path) => {
            let (startup, warning) = startup_location(&path, detect);
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            (startup, Some(path))
        }
        // Nowhere to remember a location is not a reason to stop working out
        // where the user is: the two are unrelated, and a user with no writable
        // state directory still deserves their own city.
        Err(error) => {
            eprintln!("virga: could not determine where to remember location: {error:#}");
            (
                Startup {
                    location: ActiveLocation::default(),
                    source: LocationSource::Fallback,
                    detect,
                },
                None,
            )
        }
    };

    let terminal = ratatui::init();
    let mut warning = None;
    let result = run(
        terminal,
        startup,
        theme,
        state_path.as_deref(),
        &mut warning,
    );
    ratatui::restore();
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    result
}

/// How the app should open, given what is on disk and whether detection is
/// allowed to run.
///
/// The precedence is the whole feature: a city the user chose wins outright, a
/// city that was detected before is kept only as the answer for a launch where
/// detection fails, and with neither there is New York and a lookup.
fn startup_location(path: &Path, detect: bool) -> (Startup, Option<String>) {
    let (remembered, warning) = match state::load_from(path) {
        Ok(remembered) => (remembered, None),
        Err(error) => (
            None,
            Some(format!(
                "virga: could not load remembered location: {error:#}"
            )),
        ),
    };

    let startup = match remembered {
        // A choice is a choice. Detection does not get a vote.
        Some(Remembered {
            location,
            source: LocationSource::Chosen,
        }) => Startup {
            location,
            source: LocationSource::Chosen,
            detect: false,
        },
        // Yesterday's detection is this launch's answer only if today's fails.
        Some(Remembered { location, source }) => Startup {
            location,
            source,
            detect,
        },
        None => Startup {
            location: ActiveLocation::default(),
            source: LocationSource::Fallback,
            detect,
        },
    };

    (startup, warning)
}

/// Whether to ask the network where the user is, given whatever `VIRGA_GEOIP`
/// was set to.
///
/// The `VIRGA_THEME` precedent: an unusable value is a warning and the default,
/// not an exit. Leaving detection *on* is the right default for a typo, because
/// off is the surprising state — a user who has gone to the trouble of setting
/// the variable will see the warning and fix it.
fn detection_enabled(requested: Option<&str>) -> (bool, Option<String>) {
    let Some(value) = requested else {
        return (true, None);
    };

    match value.trim().to_ascii_lowercase().as_str() {
        "off" | "0" | "false" | "no" => (false, None),
        "on" | "1" | "true" | "yes" => (true, None),
        _ => (
            true,
            Some(format!(
                "virga: VIRGA_GEOIP={value:?} is not on or off; leaving location detection on."
            )),
        ),
    }
}

/// Apply a message, persist what it asked to keep, and hand back the request it
/// chained. A detection chains the fetch that answers it.
fn accept_message(
    app: &mut App,
    message: Message,
    path: &Path,
) -> (Option<Request>, Option<String>) {
    let outcome = app.on_message(message);
    let save_warning = outcome.remember.and_then(|remembered| {
        state::save_to(path, &remembered)
            .err()
            .map(|error| format!("virga: could not remember location: {error:#}"))
    });
    // A message either keeps something or complains about something; it cannot
    // do both, so there is no ordering to get wrong here.
    (outcome.request, outcome.warning.or(save_warning))
}

fn retain_first_warning(warning: &mut Option<String>, candidate: Option<String>) {
    if warning.is_none() {
        *warning = candidate;
    }
}

/// The palette to start in, given whatever `VIRGA_THEME` was set to.
///
/// An unusable value is a warning and the default, not an exit: the variable
/// is a convenience, and refusing to show the weather because of a typo in a
/// shell profile is a poor trade.
fn startup_theme(requested: Option<&str>) -> Theme {
    let Some(name) = requested else {
        return Theme::default();
    };

    Theme::from_name(name).unwrap_or_else(|| {
        let known: Vec<&str> = Theme::ALL.into_iter().map(Theme::name).collect();
        eprintln!(
            "virga: VIRGA_THEME={name:?} is not a theme; using {}.\n       known themes: {}",
            Theme::default().name(),
            known.join(", "),
        );
        Theme::default()
    })
}

/// Hand a request to the worker without ever blocking the draw loop.
///
/// `send` on a bounded channel parks the caller until a slot frees — and the
/// caller here is the thread that owns the terminal, so the app would stop
/// drawing and stop reading keys until the network answered. `try_send` keeps
/// the loop turning and hands the refusal back to `App`, which owns what the
/// user is told.
fn dispatch(tx: &SyncSender<Request>, app: &mut App, request: Request) -> Result<()> {
    match tx.try_send(request) {
        Ok(()) => Ok(()),
        // Unreachable while the guards in `App` hold — the queue is deeper
        // than it can fill. It is handled anyway so that the day a guard is
        // dropped, the screen says something instead of waiting forever on a
        // request that was never sent.
        Err(TrySendError::Full(request)) => {
            // A dropped detection still has to reach its fallback, so the
            // replacement it asks for is dispatched rather than discarded.
            if let Some(replacement) = app.on_dispatch_dropped(request) {
                return dispatch(tx, app, replacement);
            }
            Ok(())
        }
        Err(TrySendError::Disconnected(_)) => Err(anyhow!("the worker thread has stopped")),
    }
}

/// Terminal setup, the draw loop, and carrying messages between the worker and
/// the app. Every state transition lives in `App` and every decision about what
/// a key means lives in `input`, so what remains here is the part that
/// genuinely needs a terminal and a channel.
fn run(
    mut terminal: DefaultTerminal,
    startup: Startup,
    theme: Theme,
    state_path: Option<&Path>,
    warning: &mut Option<String>,
) -> Result<()> {
    // Bounded: see `events::REQUEST_QUEUE`. Messages back stay unbounded — the
    // worker produces at most one per request it was handed, so bounding the
    // requests bounds the replies too, and a blocking send on the worker side
    // would be a deadlock waiting to happen.
    let (request_tx, request_rx) = mpsc::sync_channel(events::REQUEST_QUEUE);
    let (message_tx, message_rx) = mpsc::channel();
    events::spawn_worker(request_rx, message_tx);

    let mut app = App::with_startup(startup);
    app.theme = theme;
    let initial = app.startup_request();
    dispatch(&request_tx, &mut app, initial)?;

    let mut dirty = true;
    let mut last_size = terminal.size()?;

    loop {
        // Belt and braces against the class of bug that made resize freeze the
        // app: ratatui only reconciles its buffer inside draw(), so if the size
        // changes and nothing marks the frame dirty, a stale layout persists
        // forever. Polling it costs one ioctl per tick and does not rely on an
        // event turning up.
        let size = terminal.size()?;
        if size != last_size {
            last_size = size;
            dirty = true;
        }

        // The palette's name leaves the key bar a few seconds after `t`, and
        // nothing else would mark that frame dirty: by then the app is idle
        // and, per the rule below, an idle app draws nothing at all. So the
        // one frame that takes it back off has to be asked for here.
        if app.expire_theme_readout(Instant::now()) {
            dirty = true;
        }

        // Only the spinner and the search cursor change on their own. With
        // neither on screen there is nothing to redraw until input or a worker
        // message arrives, so an idle app costs no CPU instead of ten frames a
        // second.
        let animating = matches!(app.weather, Fetch::Loading)
            || matches!(app.results, Fetch::Loading)
            || matches!(app.screen, Screen::Search);

        if dirty || animating {
            app.tick = app.tick.wrapping_add(1);
            terminal.draw(|frame| ui::render(frame, &app))?;
            dirty = false;
        }

        while let Ok(message) = message_rx.try_recv() {
            dirty = true;
            let (chained, message_warning) = match state_path {
                Some(path) => accept_message(&mut app, message, path),
                None => {
                    let outcome = app.on_message(message);
                    (outcome.request, outcome.warning)
                }
            };
            retain_first_warning(warning, message_warning);
            // A detection answers with the fetch it asked for.
            if let Some(request) = chained {
                dispatch(&request_tx, &mut app, request)?;
            }
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                // A resize invalidates the whole buffer. The old loop redrew ten
                // times a second and papered over this; now that it only draws on
                // change, the resize has to say so itself.
                Event::Resize(_, _) => dirty = true,
                Event::Key(key) => {
                    // Keys that mean nothing on this screen — and every key
                    // release — leave no mark, so they do not even cost a redraw.
                    if let Some(action) = input::action_for(key, app.screen) {
                        dirty = true;

                        if let Some(request) = app.on_action(action) {
                            dispatch(&request_tx, &mut app, request)?;
                        }
                        if app.should_quit {
                            break Ok(());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::ActiveLocation;
    use crate::events::Message;
    use crate::weather::model::Weather;

    fn berlin() -> ActiveLocation {
        ActiveLocation {
            label: "Berlin, Germany".to_string(),
            lat: 52.52437,
            lon: 13.41053,
        }
    }

    fn loaded(request: Request) -> Message {
        let Request::Fetch { id, location } = request else {
            panic!("not a fetch")
        };
        Message::Loaded {
            id,
            location,
            weather: Weather::fixture(5, 2),
        }
    }

    fn chosen(location: ActiveLocation) -> Remembered {
        Remembered {
            location,
            source: LocationSource::Chosen,
        }
    }

    fn detected(location: ActiveLocation) -> Remembered {
        Remembered {
            location,
            source: LocationSource::Detected,
        }
    }

    fn reykjavik() -> ActiveLocation {
        ActiveLocation {
            label: "Reykjavík, Capital Region, Iceland".to_string(),
            lat: 64.146_59,
            lon: -21.942_23,
        }
    }

    /// The promise the whole feature rests on: pick a city and it is still your
    /// city next launch, wherever the network thinks you are.
    #[test]
    fn a_chosen_location_is_carried_into_startup_without_detection() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        state::save_to(&path, &chosen(berlin())).unwrap();

        let (startup, warning) = startup_location(&path, true);

        assert_eq!(startup.location, berlin());
        assert_eq!(startup.source, LocationSource::Chosen);
        assert!(!startup.detect, "a chosen city must not be re-detected");
        assert_eq!(warning, None);
    }

    /// Yesterday's guess is worth keeping only as the answer for a launch where
    /// today's lookup does not come back.
    #[test]
    fn a_detected_location_is_kept_only_as_the_fallback() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        state::save_to(&path, &detected(berlin())).unwrap();

        let (startup, _) = startup_location(&path, true);

        assert_eq!(
            startup.location,
            berlin(),
            "an offline launch still has a city"
        );
        assert_eq!(startup.source, LocationSource::Detected);
        assert!(startup.detect, "but a fresh detection outranks it");
    }

    #[test]
    fn no_state_detects_from_the_builtin_fallback() {
        let test = tempfile::tempdir().unwrap();

        let (startup, warning) = startup_location(&test.path().join("state.json"), true);

        assert_eq!(startup.location, ActiveLocation::default());
        assert_eq!(startup.source, LocationSource::Fallback);
        assert!(startup.detect);
        assert_eq!(warning, None);
    }

    #[test]
    fn opting_out_never_detects() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        state::save_to(&path, &detected(berlin())).unwrap();

        let (startup, _) = startup_location(&path, false);
        assert!(!startup.detect);
        assert_eq!(
            startup.location,
            berlin(),
            "the last guess is still the best one"
        );

        let (startup, _) = startup_location(&test.path().join("nothing.json"), false);
        assert!(!startup.detect);
        assert_eq!(startup.location, ActiveLocation::default());
    }

    #[test]
    fn broken_state_falls_back_with_a_warning() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        std::fs::write(&path, "{").unwrap();

        let (startup, warning) = startup_location(&path, true);

        assert_eq!(startup.location, ActiveLocation::default());
        assert_eq!(startup.source, LocationSource::Fallback);
        assert!(
            startup.detect,
            "an unreadable file is a reason to detect, not a reason to give up"
        );
        assert!(
            warning
                .unwrap()
                .contains("could not load remembered location")
        );
    }

    #[test]
    fn the_environment_variable_turns_detection_off() {
        for value in ["off", "Off", " OFF ", "0", "false", "no"] {
            assert!(!detection_enabled(Some(value)).0, "{value:?}");
        }
        for value in ["on", "1", "true", "yes"] {
            assert!(detection_enabled(Some(value)).0, "{value:?}");
        }
        assert!(detection_enabled(None).0);
        assert_eq!(detection_enabled(Some("off")).1, None);
    }

    /// The `VIRGA_THEME` precedent: a typo in a shell profile is a warning and
    /// the default, never a refusal to run.
    #[test]
    fn an_unusable_geoip_value_warns_and_leaves_detection_on() {
        for value in ["", "  ", "maybe", "disabled"] {
            let (enabled, warning) = detection_enabled(Some(value));

            assert!(enabled, "{value:?}");
            assert!(warning.unwrap().contains("VIRGA_GEOIP"), "{value:?}");
        }
    }

    #[test]
    fn an_accepted_load_is_persisted() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let mut app = App::with_location(berlin());
        let message = loaded(app.startup_request());

        assert_eq!(accept_message(&mut app, message, &path).1, None);
        assert_eq!(state::load_from(&path).unwrap(), Some(chosen(berlin())));
    }

    /// The end-to-end shape of a first run: detect, fetch what came back, and
    /// only then write it down — as a guess, so tomorrow detects again.
    #[test]
    fn a_detected_load_is_persisted_as_a_detection() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let mut app = App::with_startup(Startup {
            location: ActiveLocation::default(),
            source: LocationSource::Fallback,
            detect: true,
        });
        let Request::Detect { id } = app.startup_request() else {
            panic!("a first run must detect")
        };

        let (chained, warning) = accept_message(
            &mut app,
            Message::Detected {
                id,
                location: reykjavik(),
            },
            &path,
        );
        assert_eq!(warning, None);
        assert_eq!(
            state::load_from(&path).unwrap(),
            None,
            "a detection nobody has seen the weather for is not worth keeping"
        );

        let (_, warning) = accept_message(&mut app, loaded(chained.unwrap()), &path);

        assert_eq!(warning, None);
        assert_eq!(
            state::load_from(&path).unwrap(),
            Some(detected(reykjavik()))
        );
    }

    /// The wart the source field exists to fix: a first run in New York used to
    /// write New York as though the user had asked for it, which would shadow
    /// every detection after it.
    #[test]
    fn the_builtin_fallback_is_never_written_to_disk() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        let mut app = App::with_startup(Startup {
            location: ActiveLocation::default(),
            source: LocationSource::Fallback,
            detect: false,
        });
        let message = loaded(app.startup_request());

        assert_eq!(accept_message(&mut app, message, &path).1, None);
        assert_eq!(state::load_from(&path).unwrap(), None);
    }

    #[test]
    fn an_accepted_load_reports_a_nonfatal_save_failure() {
        let test = tempfile::tempdir().unwrap();
        let parent = test.path().join("not-a-directory");
        std::fs::write(&parent, "not a directory").unwrap();
        let path = parent.join("state.json");
        let mut app = App::with_location(berlin());
        let message = loaded(app.startup_request());

        let warning = accept_message(&mut app, message, &path).1.unwrap();

        assert!(warning.contains("could not remember location"));
        assert!(warning.contains("create"));
    }

    #[test]
    fn warning_retention_keeps_the_first_save_warning() {
        let mut warning = None;

        retain_first_warning(&mut warning, Some("first save failure".to_string()));
        retain_first_warning(&mut warning, Some("later save failure".to_string()));

        assert_eq!(warning.as_deref(), Some("first save failure"));
    }

    #[test]
    fn stale_and_failed_loads_do_not_replace_state() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        state::save_to(&path, &chosen(berlin())).unwrap();
        let mut app = App::new();
        let stale = app.startup_request();
        let current = app.startup_request();

        assert_eq!(accept_message(&mut app, loaded(stale), &path).1, None);
        let Request::Fetch { id, .. } = current else {
            panic!("not a fetch")
        };
        assert_eq!(
            accept_message(
                &mut app,
                Message::LoadFailed {
                    id,
                    error: "offline".to_string(),
                },
                &path,
            )
            .1,
            None
        );
        assert_eq!(state::load_from(&path).unwrap(), Some(chosen(berlin())));
    }

    #[test]
    fn no_environment_variable_means_the_default_theme() {
        assert_eq!(startup_theme(None), Theme::default());
    }

    #[test]
    fn the_environment_variable_picks_the_starting_theme() {
        for theme in Theme::ALL {
            assert_eq!(startup_theme(Some(theme.name())), theme);
        }
    }

    /// A typo in a shell profile must not stop the weather from appearing.
    #[test]
    fn an_unusable_value_falls_back_rather_than_failing() {
        for value in ["", "  ", "solarized", "Catppuccin Latte"] {
            assert_eq!(startup_theme(Some(value)), Theme::default(), "{value:?}");
        }
    }
}
