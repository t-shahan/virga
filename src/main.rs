use crate::app::{
    ActiveLocation, App, Fetch, HourlyView, LocationSource, Remembered, Screen, Startup,
};
use crate::cli::Invocation;
use crate::events::{Message, Request};
use crate::theme::{ColorDepth, Theme};
use crate::units::Unit;
use anyhow::{Context, Result, anyhow};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event;
use ratatui::crossterm::event::Event;
use std::path::Path;
use std::sync::mpsc;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::{Duration, Instant};

mod app;
mod cli;
mod events;
mod input;
mod now;
mod state;
mod theme;
mod ui;
mod units;
mod update;
mod weather;

fn main() -> Result<()> {
    // Answered before any state directory is touched or any network lookup is
    // considered: asking the version must not have side effects.
    match cli::parse_args(std::env::args().skip(1)) {
        Invocation::Run => {}
        Invocation::Version => {
            println!("virga {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Invocation::Help => {
            println!("{}", cli::usage());
            return Ok(());
        }
        // The one command that reads the state file on purpose. A file that
        // cannot be found or read still gets the list — the marker just sits
        // on the default, and the complaint goes to stderr beside it.
        Invocation::Theme(None) => {
            let (persisted, warning) = match state::path() {
                Ok(path) => load_persisted(&path),
                Err(error) => (
                    state::Persisted::default(),
                    Some(format!(
                        "virga: could not determine where themes are remembered: {error:#}"
                    )),
                ),
            };
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            print!("{}", theme_listing(persisted.theme));
            return Ok(());
        }
        Invocation::Theme(Some(name)) => {
            // Unlike VIRGA_THEME — where a typo must not stop the weather —
            // an explicit command asked a question and deserves a real
            // answer, not a fallback.
            let Some(theme) = Theme::from_name(&name) else {
                eprintln!("{}", unknown_theme_complaint(&name));
                std::process::exit(2);
            };
            match state::path().and_then(|path| state::save_theme(&path, theme)) {
                Ok(()) => {
                    println!("{}", theme_set_message(theme));
                    return Ok(());
                }
                Err(error) => {
                    eprintln!("virga: could not save the startup theme: {error:#}");
                    std::process::exit(1);
                }
            }
        }
        // The one command that fetches weather without the interface. It
        // speaks the same sources' answers in the same order of preference
        // the app opens with, so `virga now` and a launch agree about where
        // "here" is.
        Invocation::Now(city) => {
            let (unit, warning) = startup_unit(std::env::var("VIRGA_UNITS").ok().as_deref());
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            // A named city is looked up fresh, and nothing else combines
            // with it — not the state file, not detection. Asking about
            // somewhere is a question, not a move: the remembered city
            // stays whatever it was.
            let (location, freshly_detected) = match &city {
                Some(query) => match weather::client::search_locations(query) {
                    // The geocoder answered and knows no such place: the
                    // argument is wrong, which is `theme`'s unknown-name
                    // treatment. A lookup that failed to happen is the
                    // environment's fault instead, and exits like `update`'s.
                    Ok(found) => match found.first() {
                        Some(first) => (ActiveLocation::from(first), false),
                        None => {
                            eprintln!("virga: no city matched {query:?}.");
                            std::process::exit(2);
                        }
                    },
                    Err(error) => {
                        eprintln!("virga: could not search for {query:?}: {error:#}");
                        std::process::exit(1);
                    }
                },
                None => asked_location(),
            };
            match weather::client::fetch_forecast(location.lat, location.lon) {
                Ok(weather) => {
                    // The app's rule, kept here too: a detection is written
                    // down only once weather has actually loaded for it. And
                    // written down it must be — remembering the answer is
                    // what lets a status bar poll `virga now` all day without
                    // asking the location provider anything after the first.
                    if freshly_detected {
                        let remembered = Remembered {
                            location: location.clone(),
                            source: LocationSource::Detected,
                        };
                        if let Err(error) =
                            state::path().and_then(|path| state::save_location(&path, &remembered))
                        {
                            eprintln!("virga: could not remember location: {error:#}");
                        }
                    }
                    println!("{}", now::report(&location.label, &weather, unit));
                    return Ok(());
                }
                Err(error) => {
                    eprintln!("virga: could not fetch the weather: {error:#}");
                    std::process::exit(1);
                }
            }
        }
        Invocation::Update => {
            match check_for_update() {
                Ok(answer) => println!("{answer}"),
                Err(error) => {
                    eprintln!("virga: could not check the latest release: {error:#}");
                    std::process::exit(1);
                }
            }
            return Ok(());
        }
        Invocation::Usage(complaint) => {
            eprintln!("virga: {complaint}\n");
            eprintln!("{}", cli::usage());
            std::process::exit(2);
        }
        Invocation::Unknown(argument) => {
            eprintln!("virga: unrecognized argument {argument:?}\n");
            eprintln!("{}", cli::usage());
            std::process::exit(2);
        }
    }

    // All read before the terminal is taken over: a complaint about a
    // variable or the state file has to go to the ordinary screen, or it is
    // written to the alternate screen and wiped the moment the app exits.
    let (detect, warning) = detection_enabled(std::env::var("VIRGA_GEOIP").ok().as_deref());
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    let (check_updates, warning) = checks_enabled(std::env::var("VIRGA_UPDATE").ok().as_deref());
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }

    let (startup, state_path, persisted_theme) = match state::path() {
        Ok(path) => {
            let (persisted, warning) = load_persisted(&path);
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            (
                startup_location(persisted.remembered, detect),
                Some(path),
                persisted.theme,
            )
        }
        // Nowhere to remember a location is not a reason to stop working out
        // where the user is: the two are unrelated, and a user with no writable
        // state directory still deserves their own city.
        Err(error) => {
            eprintln!("virga: could not determine where to remember location: {error:#}");
            (startup_location(None, detect), None, None)
        }
    };

    let theme = startup_theme(
        std::env::var("VIRGA_THEME").ok().as_deref(),
        persisted_theme,
    );
    let (unit, warning) = startup_unit(std::env::var("VIRGA_UNITS").ok().as_deref());
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    let color_depth = ColorDepth::from_environment(
        std::env::var("COLORTERM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
    );

    let terminal = ratatui::init();
    let mut warning = None;
    let mut notice = None;
    let result = run(
        terminal,
        Opening {
            startup,
            theme,
            unit,
            color_depth,
        },
        state_path.as_deref(),
        check_updates,
        &mut warning,
        &mut notice,
    );
    ratatui::restore();
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    // News a straight-to-quit launch never gave a frame. Information, not a
    // complaint, so it goes to stdout.
    if let Some(notice) = notice {
        println!("{notice}");
    }
    result
}

/// Read the state file, folding every complaint into one warning: a document
/// that cannot be read is a warning and an empty document, never a refusal
/// to start.
fn load_persisted(path: &Path) -> (state::Persisted, Option<String>) {
    match state::load_from(path) {
        Ok(persisted) => {
            let warning = persisted.warning.clone();
            (persisted, warning)
        }
        Err(error) => (
            state::Persisted::default(),
            Some(format!("virga: could not load remembered state: {error:#}")),
        ),
    }
}

/// How the app should open, given what was remembered and whether detection
/// is allowed to run.
///
/// The precedence is the whole feature: a city the user chose wins outright, a
/// city that was detected before is kept only as the answer for a launch where
/// detection fails, and with neither there is New York and a lookup.
fn startup_location(remembered: Option<Remembered>, detect: bool) -> Startup {
    match remembered {
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
    }
}

/// The spellings of on and off an environment switch accepts, or `None` for
/// a value that is neither — the caller owns the complaint.
fn switch(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" | "0" | "false" | "no" => Some(false),
        "on" | "1" | "true" | "yes" => Some(true),
        _ => None,
    }
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

    match switch(value) {
        Some(enabled) => (enabled, None),
        None => (
            true,
            Some(format!(
                "virga: VIRGA_GEOIP={value:?} is not on or off; leaving location detection on."
            )),
        ),
    }
}

/// Whether to probe for a newer release at startup, given whatever
/// `VIRGA_UPDATE` was set to. `VIRGA_GEOIP`'s grammar and `VIRGA_GEOIP`'s
/// forgiveness, for the same reasons.
fn checks_enabled(requested: Option<&str>) -> (bool, Option<String>) {
    let Some(value) = requested else {
        return (true, None);
    };

    match switch(value) {
        Some(enabled) => (enabled, None),
        None => (
            true,
            Some(format!(
                "virga: VIRGA_UPDATE={value:?} is not on or off; leaving the update check on."
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
        state::save_location(path, &remembered)
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

/// The palette to start in, given `VIRGA_THEME` and whatever `virga theme`
/// persisted.
///
/// The environment outranks the persisted theme because a variable is set per
/// invocation, deliberately. An unusable value is a warning and the persisted
/// theme, not an exit — and not the built-in default either: the standing
/// choice absorbs the typo, because refusing to honor one setting over a typo
/// in another is a poor trade.
fn startup_theme(requested: Option<&str>, persisted: Option<Theme>) -> Theme {
    let fallback = persisted.unwrap_or_default();
    let Some(name) = requested else {
        return fallback;
    };

    Theme::from_name(name).unwrap_or_else(|| {
        let known: Vec<&str> = Theme::ALL.into_iter().map(Theme::name).collect();
        eprintln!(
            "virga: VIRGA_THEME={name:?} is not a theme; using {}.\n       known themes: {}",
            fallback.name(),
            known.join(", "),
        );
        fallback
    })
}

/// Where a bare `virga now` asks about: the remembered city, a fresh
/// detection when nothing is remembered and `VIRGA_GEOIP` allows one, and
/// the compiled-in fallback when the network will not say — a worse guess is
/// not a reason to withhold the forecast, and the stderr note says which
/// guess it was. The second value reports whether the place came from a
/// detection made just now, which is the caller's cue to remember it.
fn asked_location() -> (ActiveLocation, bool) {
    let (detect, warning) = detection_enabled(std::env::var("VIRGA_GEOIP").ok().as_deref());
    if let Some(warning) = warning {
        eprintln!("{warning}");
    }
    let remembered = match state::path() {
        Ok(path) => {
            let (persisted, warning) = load_persisted(&path);
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            persisted.remembered
        }
        // Nowhere to remember a location is not a reason to stop working
        // out where the user is — the app's judgement, applied here too.
        Err(error) => {
            eprintln!("virga: could not determine where location is remembered: {error:#}");
            None
        }
    };
    match now::where_to_ask(remembered, detect) {
        now::Ask::Location(location) => (location, false),
        now::Ask::Detect { fallback } => match weather::client::detect_location() {
            Ok(found) => (ActiveLocation::from(&found), true),
            Err(error) => {
                eprintln!(
                    "virga: could not work out where you are: {error:#}; asking about {}.",
                    fallback.label
                );
                (fallback, false)
            }
        },
    }
}

/// The measurement system to speak in, given whatever `VIRGA_UNITS` was set
/// to — for the `now` report and for the app's first frame alike, so the two
/// never disagree about what a degree is. Imperial when unset, which is the
/// system the interface has always started in; `u` still toggles it inside
/// the app for the session. `VIRGA_GEOIP`'s forgiveness for a value naming
/// neither system: a typo in a shell profile must not stop the weather.
fn startup_unit(requested: Option<&str>) -> (Unit, Option<String>) {
    let Some(name) = requested else {
        return (Unit::Imperial, None);
    };
    match Unit::from_name(name) {
        Some(unit) => (unit, None),
        None => {
            let fallback = Unit::Imperial;
            (
                fallback,
                Some(format!(
                    "virga: VIRGA_UNITS={name:?} is neither metric nor imperial; using {}.",
                    fallback.name()
                )),
            )
        }
    }
}

/// The `virga theme` listing: every theme, its character, and a marker on the
/// one the next launch will start in.
fn theme_listing(persisted: Option<Theme>) -> String {
    let startup = persisted.unwrap_or_default();
    let width = Theme::ALL
        .into_iter()
        .map(|theme| theme.name().chars().count())
        .max()
        .unwrap_or(0);

    let mut listing = String::new();
    for theme in Theme::ALL {
        let marker = if theme == startup { '*' } else { ' ' };
        let name = theme.name();
        listing.push_str(&format!("{marker} {name:width$}  {}\n", theme_blurb(theme)));
    }
    listing.push_str(
        "\nThe marked theme is the startup default. VIRGA_THEME overrides it for one\n\
         launch; `t` cycles themes inside the app for one session.\n",
    );
    listing
}

/// One line of character per theme, matching the README's table.
fn theme_blurb(theme: Theme) -> &'static str {
    match theme {
        Theme::Default => "the terminal's own sixteen colours",
        Theme::GruvboxDark => "warm — orange bars, gold selection, green today",
        Theme::Nord => "cool — icy bars, aurora-purple selection",
        Theme::TokyoNight => "blue and violet, one warm selection",
        Theme::Dracula => "loud — pink bars, lime selection, cyan today",
        Theme::CatppuccinMocha => "pastel — mauve bars, sky selection, yellow today",
        Theme::CatppuccinLatte => "the same scheme in dark ink, for light backgrounds",
    }
}

/// The whole of `virga update`: one probe, one comparison, one answer whose
/// instruction matches how this copy was installed.
fn check_for_update() -> Result<String> {
    let current = update::Release::parse(env!("CARGO_PKG_VERSION"))
        .context("parse this binary's own version")?;
    let latest = update::Release::parse(&update::latest_tag(update::RELEASES_URL)?)?;
    let exe = std::env::current_exe().ok();
    let home = directories::BaseDirs::new().map(|dirs| dirs.home_dir().to_path_buf());
    let method = update::install_method(exe.as_deref(), home.as_deref(), cfg!(windows));
    Ok(update::report(&current, &latest, &method))
}

fn theme_set_message(theme: Theme) -> String {
    format!("virga: startup theme is now {}.", theme.name())
}

fn unknown_theme_complaint(name: &str) -> String {
    let known: Vec<&str> = Theme::ALL.into_iter().map(Theme::name).collect();
    format!(
        "virga: {name:?} is not a theme.\n       known themes: {}",
        known.join(", ")
    )
}

/// Hand a request to the worker without ever blocking the draw loop.
///
/// `send` on a bounded channel parks the caller until a slot frees — and the
/// caller here is the thread that owns the terminal, so the app would stop
/// drawing and stop reading keys until the network answered. `try_send` keeps
/// the loop turning and hands the refusal back to `App`, which owns what the
/// user is told.
/// What the frame is fundamentally showing: the screen, the hourly screen's
/// view, and which of the weather states composes it. When this changes, the
/// next frame replaces the previous one wholesale rather than editing it,
/// and the draw loop clears first so the repaint is positioned absolutely.
fn composition(app: &App) -> (Screen, HourlyView, u8) {
    let weather = match &app.weather {
        Fetch::Idle => 0,
        Fetch::Loading => 1,
        Fetch::Ready(_) => 2,
        Fetch::Failed(_) => 3,
    };
    (app.screen, app.hourly_view, weather)
}

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
/// Everything the environment and the state file settled before the terminal
/// was taken over: where to open, and in which palette and units.
struct Opening {
    startup: Startup,
    theme: Theme,
    unit: Unit,
    color_depth: ColorDepth,
}

impl Opening {
    fn into_app(self) -> App {
        let mut app = App::with_startup(self.startup);
        app.theme = self.theme;
        app.unit = self.unit;
        app.color_depth = self.color_depth;
        app
    }
}

fn run(
    mut terminal: DefaultTerminal,
    opening: Opening,
    state_path: Option<&Path>,
    check_updates: bool,
    warning: &mut Option<String>,
    notice: &mut Option<String>,
) -> Result<()> {
    // Bounded: see `events::REQUEST_QUEUE`. Messages back stay unbounded — the
    // worker produces at most one per request it was handed, so bounding the
    // requests bounds the replies too, and a blocking send on the worker side
    // would be a deadlock waiting to happen.
    let (request_tx, request_rx) = mpsc::sync_channel(events::REQUEST_QUEUE);
    let (message_tx, message_rx) = mpsc::channel();
    // The probe rides its own thread and the shared message channel; the
    // worker's request queue is serial, and news must never stall a search.
    if check_updates {
        events::spawn_update_check(message_tx.clone(), || {
            let current = update::Release::parse(env!("CARGO_PKG_VERSION")).ok()?;
            let latest =
                update::Release::parse(&update::latest_tag(update::RELEASES_URL).ok()?).ok()?;
            update::notice(&current, &latest)
        });
    }
    events::spawn_worker(request_rx, message_tx);

    let mut app = opening.into_app();
    let initial = app.startup_request();
    dispatch(&request_tx, &mut app, initial)?;

    let mut dirty = true;
    let mut last_size = terminal.size()?;
    let mut last_composition = composition(&app);

    loop {
        // A wholesale change of what is on screen repaints from a clean
        // slate, the way a resize already does. An ordinary diff after one —
        // the loading popup giving way to the weather, or one screen
        // replacing another — writes long contiguous runs that trust the
        // terminal to advance exactly as far per glyph as the width tables
        // say. Terminals disagree about emoji carrying a variation selector,
        // and inside a long run that one-cell disagreement displaces
        // everything after the glyph, borders included, until the next
        // explicit cursor move. Clearing first makes every blank a run
        // break, so each fragment of the new frame is positioned absolutely.
        let now_showing = composition(&app);
        if now_showing != last_composition {
            last_composition = now_showing;
            terminal.clear()?;
            dirty = true;
        }
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

        // The first wait is the long one; after it, whatever else the burst
        // has already buffered is applied before the frame is drawn. Handled
        // one per frame, a held arrow key could outrun the draw — each repeat
        // paid for a full redraw before the next was read, so the queue grew
        // and the selection kept sliding after the key was released. The
        // frames a drain skips are ones that would never have been seen.
        let mut wait = Duration::from_millis(100);
        while event::poll(wait)? {
            wait = Duration::ZERO;
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
                            // A message already in the queue — above all the
                            // update probe's one answer, which may have
                            // landed between this frame's drain and the quit
                            // key — must not die with the receiver. Applied
                            // through the ordinary path, so a finished
                            // weather load is remembered too; only the
                            // chained request is pointless now.
                            while let Ok(message) = message_rx.try_recv() {
                                let message_warning = match state_path {
                                    Some(path) => accept_message(&mut app, message, path).1,
                                    None => app.on_message(message).warning,
                                };
                                retain_first_warning(warning, message_warning);
                            }
                            // Quit left the notice standing exactly when no
                            // other key ever cleared it; hand it out for the
                            // ordinary screen.
                            *notice = app.update_notice.take();
                            return Ok(());
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

    #[test]
    fn opening_carries_terminal_colour_depth_into_the_app() {
        let app = Opening {
            startup: Startup {
                location: ActiveLocation::default(),
                source: LocationSource::Fallback,
                detect: false,
            },
            theme: Theme::Nord,
            unit: Unit::Metric,
            color_depth: crate::theme::ColorDepth::Ansi256,
        }
        .into_app();

        assert_eq!(app.theme, Theme::Nord);
        assert_eq!(app.unit, Unit::Metric);
        assert_eq!(app.color_depth, crate::theme::ColorDepth::Ansi256);
    }

    /// The clean-repaint trigger: composition moves when the screen or the
    /// weather's state changes, and holds still through the edits a diff
    /// handles fine — a different selection, another theme, new readings
    /// inside the same Ready screen.
    #[test]
    fn composition_tracks_the_screen_and_weather_state_only() {
        let mut app = App::new();
        let loading = composition(&app);

        app.weather = Fetch::Ready(Weather::fixture(8, 0));
        let ready = composition(&app);
        assert_ne!(loading, ready, "loading giving way to weather must clear");

        app.selected_day = 3;
        app.theme = app.theme.next();
        assert_eq!(composition(&app), ready, "ordinary edits must keep diffing");

        app.weather = Fetch::Ready(Weather::fixture(8, 1));
        assert_eq!(
            composition(&app),
            ready,
            "new readings on the same screen must keep diffing"
        );

        app.screen = Screen::Hourly;
        let hourly = composition(&app);
        assert_ne!(
            hourly, ready,
            "a screen change replaces the frame wholesale"
        );

        app.hourly_view = app.hourly_view.toggle();
        assert_ne!(
            composition(&app),
            hourly,
            "flipping the hourly view replaces the frame wholesale"
        );
    }

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
        let startup = startup_location(Some(chosen(berlin())), true);

        assert_eq!(startup.location, berlin());
        assert_eq!(startup.source, LocationSource::Chosen);
        assert!(!startup.detect, "a chosen city must not be re-detected");
    }

    /// Yesterday's guess is worth keeping only as the answer for a launch where
    /// today's lookup does not come back.
    #[test]
    fn a_detected_location_is_kept_only_as_the_fallback() {
        let startup = startup_location(Some(detected(berlin())), true);

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
        let startup = startup_location(None, true);

        assert_eq!(startup.location, ActiveLocation::default());
        assert_eq!(startup.source, LocationSource::Fallback);
        assert!(startup.detect);
    }

    #[test]
    fn opting_out_never_detects() {
        let startup = startup_location(Some(detected(berlin())), false);
        assert!(!startup.detect);
        assert_eq!(
            startup.location,
            berlin(),
            "the last guess is still the best one"
        );

        let startup = startup_location(None, false);
        assert!(!startup.detect);
        assert_eq!(startup.location, ActiveLocation::default());
    }

    #[test]
    fn broken_state_falls_back_with_a_warning() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        std::fs::write(&path, "{").unwrap();

        let (persisted, warning) = load_persisted(&path);
        let startup = startup_location(persisted.remembered, true);

        assert_eq!(startup.location, ActiveLocation::default());
        assert_eq!(startup.source, LocationSource::Fallback);
        assert!(
            startup.detect,
            "an unreadable file is a reason to detect, not a reason to give up"
        );
        assert!(warning.unwrap().contains("could not load remembered state"));
    }

    /// The state module reports the parts of a document it could use and a
    /// complaint about the part it could not; the fold here has to carry that
    /// complaint out, or it dies unheard.
    #[test]
    fn a_partly_usable_state_file_keeps_its_warning() {
        let test = tempfile::tempdir().unwrap();
        let path = test.path().join("state.json");
        std::fs::write(
            &path,
            r#"{"version":2,"location":{"label":"Berlin","lat":52.0,"lon":13.0},"source":"chosen","theme":"solarized"}"#,
        )
        .unwrap();

        let (persisted, warning) = load_persisted(&path);

        assert!(persisted.remembered.is_some());
        assert_eq!(persisted.theme, None);
        assert!(warning.unwrap().contains("solarized"));
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

    /// One grammar for every switch: the update check accepts exactly the
    /// spellings detection does.
    #[test]
    fn the_environment_variable_turns_the_update_check_off() {
        for value in ["off", "Off", " OFF ", "0", "false", "no"] {
            assert!(!checks_enabled(Some(value)).0, "{value:?}");
        }
        for value in ["on", "1", "true", "yes"] {
            assert!(checks_enabled(Some(value)).0, "{value:?}");
        }
        assert!(checks_enabled(None).0);
        assert_eq!(checks_enabled(Some("off")).1, None);
    }

    #[test]
    fn an_unusable_update_value_warns_and_leaves_the_check_on() {
        let (enabled, warning) = checks_enabled(Some("maybe"));

        assert!(enabled);
        assert!(warning.unwrap().contains("VIRGA_UPDATE"));
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
        assert_eq!(
            state::load_from(&path).unwrap().remembered,
            Some(chosen(berlin()))
        );
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
            state::load_from(&path).unwrap().remembered,
            None,
            "a detection nobody has seen the weather for is not worth keeping"
        );

        let (_, warning) = accept_message(&mut app, loaded(chained.unwrap()), &path);

        assert_eq!(warning, None);
        assert_eq!(
            state::load_from(&path).unwrap().remembered,
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
        assert_eq!(state::load_from(&path).unwrap().remembered, None);
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
        state::save_location(&path, &chosen(berlin())).unwrap();
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
        assert_eq!(
            state::load_from(&path).unwrap().remembered,
            Some(chosen(berlin()))
        );
    }

    #[test]
    fn nothing_set_anywhere_means_the_default_theme() {
        assert_eq!(startup_theme(None, None), Theme::default());
    }

    #[test]
    fn the_environment_variable_picks_the_starting_theme() {
        for theme in Theme::ALL {
            assert_eq!(startup_theme(Some(theme.name()), None), theme);
        }
    }

    #[test]
    fn the_persisted_theme_outranks_the_default() {
        assert_eq!(startup_theme(None, Some(Theme::Nord)), Theme::Nord);
    }

    /// A variable is set per invocation, deliberately; the persisted theme is
    /// the standing default it overrides.
    #[test]
    fn the_environment_outranks_the_persisted_theme() {
        assert_eq!(
            startup_theme(Some("dracula"), Some(Theme::Nord)),
            Theme::Dracula
        );
    }

    /// A typo in a shell profile must not stop the weather from appearing.
    #[test]
    fn an_unusable_value_falls_back_rather_than_failing() {
        for value in ["", "  ", "solarized", "Catppuccin Frappe"] {
            assert_eq!(
                startup_theme(Some(value), None),
                Theme::default(),
                "{value:?}"
            );
        }
    }

    /// The standing choice absorbs the typo: refusing to honor one setting
    /// over a mistake in another would be a poor trade.
    #[test]
    fn an_unusable_environment_value_falls_back_to_the_persisted_theme() {
        assert_eq!(
            startup_theme(Some("solarized"), Some(Theme::Nord)),
            Theme::Nord
        );
    }

    /// Imperial is what the interface has always started in; the variable
    /// exists so nobody metric has to press `u` every launch.
    #[test]
    fn units_default_to_imperial_and_the_variable_switches_them() {
        assert_eq!(startup_unit(None), (Unit::Imperial, None));
        assert_eq!(startup_unit(Some("metric")), (Unit::Metric, None));
        assert_eq!(startup_unit(Some("imperial")), (Unit::Imperial, None));
    }

    /// A typo in a shell profile must not stop the weather: the default and
    /// a warning, never an exit.
    #[test]
    fn an_unusable_units_value_warns_and_stays_imperial() {
        for value in ["", "  ", "kelvin", "both"] {
            let (unit, warning) = startup_unit(Some(value));

            assert_eq!(unit, Unit::Imperial, "{value:?}");
            assert!(warning.unwrap().contains("VIRGA_UNITS"), "{value:?}");
        }
    }

    #[test]
    fn the_listing_names_every_theme_once() {
        let listing = theme_listing(None);
        for theme in Theme::ALL {
            assert!(
                listing.contains(theme.name()),
                "{} is missing",
                theme.name()
            );
        }
    }

    /// The marker is the answer to "what will the next launch look like":
    /// it sits on the persisted theme, or on the built-in default when
    /// nothing was ever persisted.
    #[test]
    fn the_marker_sits_on_the_startup_default() {
        let marked = |listing: &str| -> String {
            listing
                .lines()
                .find(|line| line.starts_with('*'))
                .expect("no line is marked")
                .to_string()
        };

        assert!(marked(&theme_listing(None)).contains("default"));
        assert!(marked(&theme_listing(Some(Theme::TokyoNight))).contains("tokyo night"));
    }

    #[test]
    fn setting_a_theme_confirms_it_by_name() {
        assert!(theme_set_message(Theme::Nord).contains("nord"));
    }

    /// The complaint an explicit `virga theme` typo earns has to leave the
    /// user able to fix it without opening the README.
    #[test]
    fn an_unknown_theme_complaint_lists_what_would_have_worked() {
        let complaint = unknown_theme_complaint("solarized");
        assert!(complaint.contains("solarized"));
        for theme in Theme::ALL {
            assert!(
                complaint.contains(theme.name()),
                "{} is missing",
                theme.name()
            );
        }
    }
}
