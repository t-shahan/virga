use crate::events::{Message, Request, RequestId};
use crate::input::Action;
use crate::theme::{ColorDepth, Theme};
use crate::units::Unit;
use crate::weather::model::{Location, Weather};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

pub enum Fetch<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Weather,
    Search,
    Hourly,
}

/// Which rendering the hourly screen uses. The weathergram is the default;
/// the classic view is the precipitation-centred screen it replaced, kept so
/// the choice belongs to the user rather than to the release. Session-scoped
/// deliberately: a view is a way of looking, not a setting, and the toggle is
/// one keypress away.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HourlyView {
    #[default]
    Weathergram,
    Classic,
}

impl HourlyView {
    pub fn toggle(self) -> Self {
        match self {
            HourlyView::Weathergram => HourlyView::Classic,
            HourlyView::Classic => HourlyView::Weathergram,
        }
    }
}

/// How far the vertical arrows jump. Eight days is a long way at one press
/// per hour.
const HOURS_PER_DAY: usize = 24;

/// A place the app can fetch for, with its label and coordinates in one value.
/// They were separate before: selecting a search result stored the label and
/// discarded the coordinates, so refresh silently fell back to the default city
/// while the border kept showing the chosen one.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ActiveLocation {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
}

impl Default for ActiveLocation {
    fn default() -> Self {
        Self {
            label: "New York City, New York, United States".to_string(),
            lat: 40.7128,
            lon: -74.0060,
        }
    }
}

impl From<&Location> for ActiveLocation {
    fn from(found: &Location) -> Self {
        Self {
            label: found.label(),
            lat: found.lat,
            lon: found.lon,
        }
    }
}

/// Why the active location is the active location.
///
/// The location alone cannot answer that, and the answer decides two things:
/// what is written to disk, and what a later launch is allowed to override. A
/// file that records only coordinates cannot tell a city you went looking for
/// from one the app guessed at, which is how the compiled-in default used to be
/// saved as though you had asked for it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LocationSource {
    /// Picked in the search. Nothing outranks it — not a later detection, not
    /// anything.
    Chosen,
    /// Resolved from the IP address at launch. Written so an offline launch has
    /// a city to show, but never pinned: the next launch detects again.
    Detected,
    /// The compiled-in default, shown when there is nothing better. Deliberately
    /// never written — saving it would let a first run masquerade as a choice
    /// and shadow every detection after it.
    Fallback,
}

/// How the app opens: the place to show if nothing better arrives, where that
/// place came from, and whether to ask the network who we are first.
pub struct Startup {
    pub location: ActiveLocation,
    pub source: LocationSource,
    pub detect: bool,
}

/// A location worth keeping, and the provenance that decides whether a later
/// launch may replace it.
#[derive(Clone, Debug, PartialEq)]
pub struct Remembered {
    pub location: ActiveLocation,
    pub source: LocationSource,
}

/// What a worker message asked the caller to do: what to write to disk, and
/// what to send next.
///
/// Returning the request rather than issuing it keeps `App` free of the
/// channel, exactly as `on_action` already is.
#[derive(Default)]
pub struct Outcome {
    pub remember: Option<Remembered>,
    pub request: Option<Request>,
    /// Something worth saying that is not worth a screen. Printed once the
    /// terminal has been given back, like the state-file warnings — a failed
    /// detection is recoverable by definition, so it must not interrupt the
    /// weather, but a user who wonders why they are in New York deserves the
    /// reason on their way out.
    pub warning: Option<String>,
}

impl Outcome {
    fn nothing() -> Self {
        Self::default()
    }
}

/// The weather request in flight: which one it is, the place it is for, and
/// where that place came from. Refresh and retry aim at the place, so a failed
/// switch retries the city you asked for rather than the one still on screen —
/// and it carries the source so an accepted response is saved as what it is.
struct Pending {
    id: RequestId,
    location: ActiveLocation,
    source: LocationSource,
}

pub struct App {
    pub screen: Screen,
    pub query: String,
    pub results: Fetch<Vec<Location>>,
    pub weather: Fetch<Weather>,
    pub unit: Unit,
    /// The palette in use. A name only — resolving it to colours is `ui`'s
    /// business, which is what keeps this module free of Ratatui types.
    pub theme: Theme,
    /// The colour vocabulary inferred for the terminal at startup. Kept apart
    /// from the theme so cycling changes the design, not the capability.
    pub color_depth: ColorDepth,
    pub tick: usize,
    pub selected: usize,
    /// Index into `Weather::daily` of the day being inspected. Distinct from
    /// `selected`, which tracks the search results list.
    pub selected_day: usize,
    /// Index into `Weather::forecast_hours()` — the hourly series from now
    /// onward, so zero is always the current hour.
    pub selected_hour: usize,
    /// Which rendering the hourly screen uses, toggled with `v`.
    pub hourly_view: HourlyView,
    /// The place the displayed weather actually describes. Only a successful
    /// load moves it, so the label can never get ahead of the measurements.
    pub location: ActiveLocation,
    /// Where `location` came from. Moves with it, and only with it.
    location_source: LocationSource,
    /// Whether the startup request should ask the network where we are before
    /// fetching anything.
    detect_at_startup: bool,
    pending: Option<Pending>,
    /// The detection in flight. Kept apart from `pending` so a detection and a
    /// weather fetch can never be mistaken for one another, the same way the
    /// search already is.
    pending_detect: Option<RequestId>,
    /// The search request in flight. Editing the query abandons it, so a slow
    /// response cannot arrive and repopulate results for a query you have
    /// already moved on from.
    pending_search: Option<RequestId>,
    next_request: RequestId,
    /// Set by `Action::Quit`; the event loop reads it and stops.
    pub should_quit: bool,
    /// The screen the search was opened from, and the one leaving it returns
    /// to. Searching from the hourly screen used to land you on the
    /// weather screen regardless.
    search_return: Screen,
    /// When the palette's name stops being shown beside `t` on the key bar.
    /// `None` once it has lapsed, and before `t` is ever pressed.
    ///
    /// The name answers "which one did I just land on", which is a question
    /// only worth answering while you are cycling. Left up permanently it is
    /// a status readout nobody is reading, holding columns the bar can put to
    /// better use — and on a narrow terminal it costs a whole binding.
    theme_readout_until: Option<Instant>,
    /// The one line of release news, when the background probe found any.
    /// Composed in the probe, so this module never learns about versions,
    /// paths, or the network — it holds a string and lets it go.
    pub update_notice: Option<String>,
}

/// How long the palette's name stays on the key bar after `t`. Long enough to
/// read at a glance, short enough that it is gone before you would notice it
/// sitting there.
const THEME_READOUT: Duration = Duration::from_secs(3);

impl App {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::with_location(ActiveLocation::default())
    }

    /// An app that starts at one known place and asks nothing of the network
    /// beyond its weather.
    #[cfg(test)]
    pub fn with_location(location: ActiveLocation) -> Self {
        Self::with_startup(Startup {
            location,
            source: LocationSource::Chosen,
            detect: false,
        })
    }

    pub fn with_startup(startup: Startup) -> Self {
        let Startup {
            location,
            source,
            detect,
        } = startup;
        Self {
            screen: Screen::Weather,
            query: String::new(),
            results: Fetch::Idle,
            weather: Fetch::Loading,
            unit: Unit::Imperial,
            theme: Theme::default(),
            color_depth: ColorDepth::Ansi16,
            tick: 0,
            selected: 0,
            selected_day: 0,
            selected_hour: 0,
            hourly_view: HourlyView::default(),
            location,
            location_source: source,
            detect_at_startup: detect,
            pending: None,
            pending_detect: None,
            pending_search: None,
            next_request: 0,
            should_quit: false,
            search_return: Screen::Weather,
            theme_readout_until: None,
            update_notice: None,
        }
    }

    /// The request that fills the first frame.
    ///
    /// A city the user chose is fetched at once — detection does not get a vote
    /// on a decision already made. Anything else asks where we are first, and
    /// chains the fetch onto the answer.
    pub fn startup_request(&mut self) -> Request {
        if self.detect_at_startup && self.location_source != LocationSource::Chosen {
            let id = self.next_id();
            self.pending_detect = Some(id);
            return Request::Detect { id };
        }

        let location = self.location.clone();
        self.fetch(location, self.location_source)
    }

    /// Apply an action, and hand back the request it wants making. Keeping the
    /// I/O out here — the caller owns the channel — is what lets every
    /// transition below be tested without a terminal or a network.
    pub fn on_action(&mut self, action: Action) -> Option<Request> {
        // The notice is dismissed by living — but only by a key that could
        // have seen it. The search screen never renders the notice, so keys
        // pressed there must not silently delete news nobody was shown; and
        // quit keeps it, so the event loop can hand it back for the ordinary
        // screen a straight-to-quit launch never gave it a frame on.
        if !matches!(action, Action::Quit) && self.screen != Screen::Search {
            self.update_notice = None;
        }
        match action {
            Action::Quit => self.should_quit = true,
            Action::Back => self.back(),
            Action::Refresh => return self.refresh(),
            Action::Submit => return self.submit(),
            Action::ToggleUnits => self.unit = self.unit.toggle(),
            Action::CycleTheme => {
                self.theme = self.theme.next();
                self.theme_readout_until = Some(Instant::now() + THEME_READOUT);
            }
            Action::OpenSearch => self.open_search(),
            Action::OpenHourly => {
                self.screen = Screen::Hourly;
                self.select_now();
            }
            Action::PrevDay => self.select_prev_day(),
            Action::NextDay => self.select_next_day(),
            Action::Today => self.select_today(),
            Action::PrevHour => self.select_prev_hour(),
            Action::NextHour => self.select_next_hour(),
            Action::PrevHourDay => self.select_prev_hour_day(),
            Action::NextHourDay => self.select_next_hour_day(),
            Action::Now => self.select_now(),
            Action::ToggleHourlyView => self.hourly_view = self.hourly_view.toggle(),
            Action::Insert(c) => {
                self.query.push(c);
                self.invalidate_results();
            }
            Action::Backspace => {
                self.query.pop();
                self.invalidate_results();
            }
            Action::PrevResult => self.selected = self.selected.saturating_sub(1),
            Action::NextResult => {
                if let Fetch::Ready(locations) = &self.results
                    && self.selected + 1 < locations.len()
                {
                    self.selected += 1;
                }
            }
        }
        None
    }

    /// Apply a worker message, reporting what to keep and what to send next.
    ///
    /// Only an accepted weather load is worth keeping. Ignored, failed and
    /// search responses keep nothing; a detection sends a fetch without keeping
    /// anything, because a place nobody has seen the weather for is not yet a
    /// place worth remembering.
    ///
    /// The worker's ordering is not an identity guarantee: a slow first
    /// response must not overwrite a fast second one, and a search whose query
    /// has since changed must not repopulate the list.
    pub fn on_message(&mut self, message: Message) -> Outcome {
        match message {
            // No id and no staleness: at most one is ever sent, and news
            // about a release does not go stale over a session.
            Message::UpdateAvailable { notice } => {
                self.update_notice = Some(notice);
                Outcome::nothing()
            }
            Message::Loaded {
                id,
                location,
                weather,
            } => {
                if !self.awaiting_weather(id) {
                    return Outcome::nothing();
                }
                let source = self
                    .pending
                    .take()
                    .map_or(self.location_source, |p| p.source);
                self.selected_day = weather.today_index;
                self.selected_hour = 0;
                self.location = location;
                self.location_source = source;
                self.weather = Fetch::Ready(weather);
                Outcome {
                    remember: self.remembered(),
                    ..Outcome::nothing()
                }
            }
            // `pending` deliberately survives a failure, so retrying aims at
            // the place that failed rather than the one still on screen.
            Message::LoadFailed { id, error } => {
                if self.awaiting_weather(id) {
                    self.weather = Fetch::Failed(error);
                }
                Outcome::nothing()
            }
            Message::Detected { id, location } => {
                if !self.awaiting_detection(id) {
                    return Outcome::nothing();
                }
                self.pending_detect = None;
                Outcome {
                    request: Some(self.fetch(location, LocationSource::Detected)),
                    ..Outcome::nothing()
                }
            }
            // Not an error screen. A detection that does not answer costs the
            // user a better guess, not their forecast: the fallback the app
            // opened with is fetched instead, and if that fails too it is the
            // weather that says so.
            Message::DetectFailed { id, error } => {
                if !self.awaiting_detection(id) {
                    return Outcome::nothing();
                }
                self.pending_detect = None;
                let location = self.location.clone();
                let warning = format!(
                    "virga: could not work out where you are: {error}\n       starting in {} — press l to search",
                    location.label
                );
                Outcome {
                    request: Some(self.fetch(location, self.location_source)),
                    warning: Some(warning),
                    ..Outcome::nothing()
                }
            }
            Message::Located { id, locations } => {
                if self.awaiting_search(id) {
                    self.pending_search = None;
                    self.results = Fetch::Ready(locations);
                }
                Outcome::nothing()
            }
            Message::SearchFailed { id, error } => {
                if self.awaiting_search(id) {
                    self.pending_search = None;
                    self.results = Fetch::Failed(error);
                }
                Outcome::nothing()
            }
        }
    }

    /// Whether the app is still working out where the user is, rather than
    /// fetching weather for somewhere it already knows.
    pub fn is_locating(&self) -> bool {
        self.pending_detect.is_some()
    }

    /// The active location, when it is one worth writing down. The compiled-in
    /// fallback never is.
    fn remembered(&self) -> Option<Remembered> {
        match self.location_source {
            LocationSource::Fallback => None,
            source => Some(Remembered {
                location: self.location.clone(),
                source,
            }),
        }
    }

    /// Whether the key bar should still name the palette beside `t`.
    pub fn theme_readout_visible(&self) -> bool {
        self.theme_readout_until.is_some()
    }

    /// Drop the readout if its moment has passed, reporting whether it did.
    ///
    /// The caller owns the clock rather than this asking for the time itself,
    /// which is what lets a test skip three seconds instead of sleeping them.
    /// The answer matters because the draw loop only redraws when something
    /// says it must: by the time this lapses the app is idle and drawing
    /// nothing, so the frame that takes the name back off the bar happens only
    /// if this returns `true`.
    pub fn expire_theme_readout(&mut self, now: Instant) -> bool {
        match self.theme_readout_until {
            Some(deadline) if now >= deadline => {
                self.theme_readout_until = None;
                true
            }
            _ => false,
        }
    }

    /// A request this app asked for that never reached the worker, because the
    /// bounded queue had no room for it.
    ///
    /// Every path that issues a request has already moved the screen to
    /// `Loading` and recorded the id as pending. Dropping the request without
    /// telling anyone would leave both in place with no response ever coming —
    /// a spinner that never stops. Reporting it as a failure is what keeps the
    /// bound from turning a full queue into a hang.
    pub fn on_dispatch_dropped(&mut self, request: Request) -> Option<Request> {
        match request {
            // `pending` survives, exactly as it does for `LoadFailed`, so the
            // retry aims at the place that failed rather than the one on screen.
            Request::Fetch { id, .. } => {
                if self.awaiting_weather(id) {
                    self.weather = Fetch::Failed("too many requests at once".to_string());
                }
            }
            // A dropped detection is the detection failing, and it fails the
            // same way: on to the fallback rather than into an error screen.
            // Without this the app would sit on "locating..." forever.
            Request::Detect { id } => {
                if self.awaiting_detection(id) {
                    self.pending_detect = None;
                    let location = self.location.clone();
                    return Some(self.fetch(location, self.location_source));
                }
            }
            Request::Search { id, .. } => {
                if self.awaiting_search(id) {
                    self.pending_search = None;
                    self.results = Fetch::Failed("too many requests at once".to_string());
                }
            }
        }
        None
    }

    fn awaiting_weather(&self, id: RequestId) -> bool {
        self.pending.as_ref().is_some_and(|p| p.id == id)
    }

    fn awaiting_detection(&self, id: RequestId) -> bool {
        self.pending_detect == Some(id)
    }

    fn awaiting_search(&self, id: RequestId) -> bool {
        self.pending_search == Some(id)
    }

    fn next_id(&mut self) -> RequestId {
        self.next_request += 1;
        self.next_request
    }

    /// Aim at a place. The label does not move yet — only a response does that.
    fn fetch(&mut self, location: ActiveLocation, source: LocationSource) -> Request {
        let id = self.next_id();
        self.pending = Some(Pending {
            id,
            location: location.clone(),
            source,
        });
        self.weather = Fetch::Loading;
        Request::Fetch { id, location }
    }

    /// Ignored while a fetch is already running, so a held `r` cannot queue one
    /// request per keypress against a single worker.
    fn refresh(&mut self) -> Option<Request> {
        if matches!(self.weather, Fetch::Loading) {
            return None;
        }
        let (target, source) = self.refresh_target();
        Some(self.fetch(target, source))
    }

    /// Enter either opens the highlighted result or runs the query, depending
    /// on whether there is a result to open.
    fn submit(&mut self) -> Option<Request> {
        let picked = match &self.results {
            Fetch::Ready(locations) => locations.get(self.selected).map(ActiveLocation::from),
            _ => None,
        };

        if let Some(picked) = picked {
            self.results = Fetch::Idle;
            self.close_search();
            // The one place a location becomes a choice. Everything else the app
            // fetches is a guess it made on the user's behalf.
            return Some(self.fetch(picked, LocationSource::Chosen));
        }

        // A repeated Enter while the same query is already running would queue
        // a duplicate search behind it.
        if self.query.is_empty() || matches!(self.results, Fetch::Loading) {
            return None;
        }

        let id = self.next_id();
        self.pending_search = Some(id);
        self.results = Fetch::Loading;
        self.selected = 0;
        Some(Request::Search {
            id,
            query: self.query.clone(),
        })
    }

    /// Leaving a screen. The search returns where it came from; everything
    /// else returns to the weather.
    fn back(&mut self) {
        self.screen = match self.screen {
            Screen::Search => self.search_return,
            _ => Screen::Weather,
        };
    }

    /// What `r` should fetch: whatever we are already chasing, else what is on
    /// screen. Never the compiled-in default — that was the bug. The source
    /// rides along, so a refresh cannot quietly relabel where a place came from.
    fn refresh_target(&self) -> (ActiveLocation, LocationSource) {
        self.pending.as_ref().map_or_else(
            || (self.location.clone(), self.location_source),
            |p| (p.location.clone(), p.source),
        )
    }

    /// Both directions wrap, so the window is a loop rather than a corridor.
    pub fn select_prev_day(&mut self) {
        if let Some(last) = self.last_day() {
            self.selected_day = if self.selected_day == 0 {
                last
            } else {
                self.selected_day - 1
            };
        }
    }

    pub fn select_next_day(&mut self) {
        if let Some(last) = self.last_day() {
            self.selected_day = if self.selected_day >= last {
                0
            } else {
                self.selected_day + 1
            };
        }
    }

    /// `None` while there is no forecast to move around in.
    fn last_day(&self) -> Option<usize> {
        match &self.weather {
            Fetch::Ready(weather) if !weather.daily.is_empty() => Some(weather.daily.len() - 1),
            _ => None,
        }
    }

    /// Jump back to today. Arrowing home from two weeks out is not a journey
    /// anyone should have to make.
    pub fn select_today(&mut self) {
        if let Fetch::Ready(weather) = &self.weather {
            self.selected_day = weather.today_index;
        }
    }

    /// `None` while there is no hourly forecast to move around in — the same
    /// guard `last_day` gives the day arrows.
    fn hour_count(&self) -> Option<usize> {
        match &self.weather {
            Fetch::Ready(weather) => match weather.forecast_hours().len() {
                0 => None,
                count => Some(count),
            },
            _ => None,
        }
    }

    /// One step of `delta` hours, wrapping at both ends as the day arrows do.
    /// `rem_euclid` is what makes a backwards step from hour zero land on the
    /// last hour rather than underflowing.
    fn step_hour(&mut self, delta: isize) {
        let Some(count) = self.hour_count() else {
            return;
        };
        let count = count as isize;
        self.selected_hour = (self.selected_hour as isize + delta).rem_euclid(count) as usize;
    }

    /// The fixture's window is a whole number of days, which is exactly the
    /// case the day-step bug hid behind, so tests build partial ones too.
    #[cfg(test)]
    fn forecast_hour_stamps(&self) -> Vec<String> {
        match &self.weather {
            Fetch::Ready(weather) => weather
                .forecast_hours()
                .iter()
                .map(|h| h.time.clone())
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn select_prev_hour(&mut self) {
        self.step_hour(-1);
    }

    pub fn select_next_hour(&mut self) {
        self.step_hour(1);
    }

    /// A day's step, wrapping to the *same time of day* at the other end.
    ///
    /// The forward window is however many hours are left of the forecast, so it
    /// is only a whole number of days at the top of the hour it was fetched —
    /// 173 hours, say, which is seven days and five hours. Wrapping on the raw
    /// index therefore shifted the clock by those five hours every lap, walking
    /// 7 PM to 2 PM to 9 AM. The days it landed on looked arbitrary because the
    /// time of day was silently sliding. Wrapping within the hours that share a
    /// time of day keeps it pinned.
    fn step_day(&mut self, forward: bool) {
        let Some(count) = self.hour_count() else {
            return;
        };
        let time_of_day = self.selected_hour % HOURS_PER_DAY;

        self.selected_hour = if forward {
            match self.selected_hour + HOURS_PER_DAY {
                next if next < count => next,
                // Back to the first hour sharing this time of day.
                _ => time_of_day,
            }
        } else if let Some(previous) = self.selected_hour.checked_sub(HOURS_PER_DAY) {
            previous
        } else if self.selected_hour >= self.hours_left_today() {
            // Stepping back onto today, which the window opened partway
            // through. A clock hour earlier than the one it opened at has no
            // cell in that first day, so the step lands on its earliest hour —
            // now — rather than preserving a time of day that day cannot
            // offer.
            //
            // Preserving it is what the rest of this function exists to do, and
            // it is right everywhere else. Here it made the first day
            // unreachable by the day arrows from eighteen of the twenty-four
            // clock hours: the cycle `t, t+24, t+48…` only ever touches the
            // first day when `t` is one of the few hours left in it. Left and
            // right could still walk onto today an hour at a time, which is
            // what made the gap look like a rendering fault in the week strip
            // rather than a navigation one.
            0
        } else {
            // Already on the first day. The last day that reaches this time of
            // day; the final day of the window is usually partial, so that is
            // not always the last day.
            let days = (count - 1 - time_of_day) / HOURS_PER_DAY;
            time_of_day + days * HOURS_PER_DAY
        };
    }

    /// Hours of the current day the forward window still holds.
    ///
    /// The window opens at the current hour, so its first day is a partial one
    /// — six hours of it at 6 PM — and it is the only day whose clock hours do
    /// not run 0 to 23. Indices below this are today; the rest are whole days.
    fn hours_left_today(&self) -> usize {
        let Fetch::Ready(weather) = &self.weather else {
            return HOURS_PER_DAY;
        };

        weather
            .forecast_hours()
            .first()
            .and_then(|hour| hour.time.get(11..13))
            .and_then(|clock| clock.parse::<usize>().ok())
            .filter(|clock| *clock < HOURS_PER_DAY)
            .map_or(HOURS_PER_DAY, |clock| HOURS_PER_DAY - clock)
    }

    pub fn select_prev_hour_day(&mut self) {
        self.step_day(false);
    }

    pub fn select_next_hour_day(&mut self) {
        self.step_day(true);
    }

    /// Hour zero of the forward window is the current hour, by construction.
    pub fn select_now(&mut self) {
        self.selected_hour = 0;
    }

    /// Open the city search, remembering where it was opened from. Searching
    /// is a detour rather than a way home: coming back should return you to
    /// the screen you left, not always to the weather.
    ///
    /// The old query and its results go with it. Results describe the query
    /// that produced them, and a fresh search starts with neither — without
    /// this, Enter on the empty query would reopen a city from the last search.
    pub fn open_search(&mut self) {
        self.search_return = self.screen;
        self.screen = Screen::Search;
        self.query.clear();
        self.invalidate_results();
    }

    /// Leave the search, whether a city was chosen or the search abandoned.
    pub fn close_search(&mut self) {
        self.screen = self.search_return;
    }

    /// Results only describe the query that produced them, so any edit to the
    /// query discards them. Without this, Enter keeps taking the "select"
    /// branch and opens a city from the previous search instead of running a
    /// new one.
    pub fn invalidate_results(&mut self) {
        self.results = Fetch::Idle;
        self.selected = 0;
        // Abandon the request too, not just its results. A search already in
        // flight would otherwise land after the edit and repopulate the list
        // for a query that is no longer on screen.
        self.pending_search = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hourly_view_toggles_between_weathergram_and_classic() {
        let mut app = App::new();
        assert_eq!(app.hourly_view, HourlyView::Weathergram);
        app.on_action(Action::ToggleHourlyView);
        assert_eq!(app.hourly_view, HourlyView::Classic);
        app.on_action(Action::ToggleHourlyView);
        assert_eq!(app.hourly_view, HourlyView::Weathergram);
    }

    fn app_with(days: usize, today: usize) -> App {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(days, today));
        app.selected_day = today;
        app
    }

    /// Changing the palette is a local change to one field. It must not move
    /// the selection, touch the units, leave the screen, or — above all — send
    /// anything to the network.
    #[test]
    fn cycling_the_theme_changes_nothing_else() {
        let mut app = app_with(22, 14);
        app.screen = Screen::Hourly;
        app.selected_hour = 7;

        let before = app.theme;
        let request = app.on_action(Action::CycleTheme);

        assert!(request.is_none(), "a theme change asked for a fetch");
        assert_eq!(app.theme, before.next());
        assert_eq!(app.screen, Screen::Hourly);
        assert_eq!(app.selected_day, 14);
        assert_eq!(app.selected_hour, 7);
        assert_eq!(app.unit, Unit::Imperial);
    }

    /// The name is feedback on a press, so it starts hidden — nothing has been
    /// answered yet — appears when `t` answers it, and goes once the answer has
    /// been read.
    #[test]
    fn the_theme_readout_shows_on_a_press_and_lapses_on_its_own() {
        let mut app = app_with(22, 14);
        assert!(
            !app.theme_readout_visible(),
            "the bar named a palette nobody had asked about"
        );

        app.on_action(Action::CycleTheme);
        assert!(app.theme_readout_visible(), "pressing t said nothing");

        assert!(
            !app.expire_theme_readout(Instant::now()),
            "the readout lapsed the instant it appeared"
        );
        assert!(app.theme_readout_visible());

        assert!(
            app.expire_theme_readout(Instant::now() + THEME_READOUT * 2),
            "expiring should report that it changed something"
        );
        assert!(!app.theme_readout_visible());
    }

    /// The draw loop only redraws when something says it must, and it asks this
    /// on every pass. Reporting a change when there was none would put the app
    /// back to redrawing on a timer, which is the thing the idle path exists to
    /// avoid.
    #[test]
    fn an_expired_readout_stops_asking_for_redraws() {
        let mut app = app_with(22, 14);
        let long_past = Instant::now() + THEME_READOUT * 2;

        assert!(
            !app.expire_theme_readout(long_past),
            "a readout that was never shown reported a change"
        );

        app.on_action(Action::CycleTheme);
        assert!(app.expire_theme_readout(long_past));

        for _ in 0..3 {
            assert!(
                !app.expire_theme_readout(long_past),
                "an already-lapsed readout kept asking to be redrawn"
            );
        }
    }

    /// Each press restarts the clock, so cycling through several palettes
    /// leaves the name up for a few seconds after the *last* one rather than
    /// the first.
    #[test]
    fn pressing_again_restarts_the_readout() {
        let mut app = app_with(22, 14);

        app.on_action(Action::CycleTheme);
        let after_first = app.theme_readout_until.expect("the first press showed it");

        app.on_action(Action::CycleTheme);
        let after_second = app.theme_readout_until.expect("the second press showed it");

        assert!(after_second >= after_first, "the second press cut it short");
    }

    /// Five presses is a lap, so a user who cycles past the one they wanted can
    /// keep pressing rather than having to know a way back.
    #[test]
    fn cycling_all_the_way_round_returns_to_the_starting_theme() {
        let mut app = app_with(22, 14);
        let start = app.theme;

        for _ in 0..Theme::ALL.len() {
            app.on_action(Action::CycleTheme);
        }

        assert_eq!(app.theme, start);
    }

    #[test]
    fn next_day_wraps_past_the_end() {
        let mut app = app_with(5, 2);
        app.selected_day = 4;
        app.select_next_day();
        assert_eq!(app.selected_day, 0);
    }

    #[test]
    fn prev_day_wraps_before_the_start() {
        let mut app = app_with(5, 2);
        app.selected_day = 0;
        app.select_prev_day();
        assert_eq!(app.selected_day, 4);
    }

    #[test]
    fn stepping_forward_and_back_returns_to_the_same_day() {
        let mut app = app_with(22, 14);
        for start in [0, 1, 14, 21] {
            app.selected_day = start;
            app.select_next_day();
            app.select_prev_day();
            assert_eq!(app.selected_day, start, "round trip from {start}");
        }
    }

    #[test]
    fn a_full_lap_returns_to_the_start() {
        let mut app = app_with(22, 14);
        for _ in 0..22 {
            app.select_next_day();
        }
        assert_eq!(app.selected_day, 14);
    }

    #[test]
    fn select_today_returns_to_the_current_day() {
        let mut app = app_with(22, 14);
        app.selected_day = 3;
        app.select_today();
        assert_eq!(app.selected_day, 14);
    }

    /// Arrow keys are live before the first forecast lands; they must not panic
    /// or leave the index pointing at a day that does not exist.
    #[test]
    fn navigation_is_inert_without_a_forecast() {
        let mut app = App::new();
        app.select_next_day();
        app.select_prev_day();
        app.select_today();
        assert_eq!(app.selected_day, 0);
    }

    #[test]
    fn navigation_is_inert_with_an_empty_forecast() {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(0, 0));
        app.select_next_day();
        app.select_prev_day();
        assert_eq!(app.selected_day, 0);
    }

    /// The fixture carries 24 hours of history before `now_hour`, so anything
    /// that leaks the past into this window shows up as an off-by-24.
    #[test]
    fn the_hour_window_opens_at_now_and_looks_only_forward() {
        let app = app_with(22, 14);
        let Fetch::Ready(w) = &app.weather else {
            panic!("fixture is ready")
        };

        assert_eq!(app.selected_hour, 0);
        assert_eq!(w.forecast_hours().len(), 192, "eight days ahead");
        assert_eq!(w.forecast_hours()[0].time, w.hourly[w.now_hour].time);
    }

    #[test]
    fn hour_navigation_wraps_at_both_ends() {
        let mut app = app_with(22, 14);

        app.select_prev_hour();
        assert_eq!(app.selected_hour, 191, "back from now lands on the last");

        app.select_next_hour();
        assert_eq!(app.selected_hour, 0, "and forward again returns");
    }

    /// A window of 173 hours — seven days and five — is the ordinary case: it
    /// is however much forecast is left at whatever hour you happened to open
    /// the app. The fixture's 192 is the lucky one, which is why this bug
    /// survived a full test suite.
    fn app_with_a_partial_last_day() -> App {
        let mut app = app_with(22, 14);
        if let Fetch::Ready(weather) = &mut app.weather {
            weather.hourly.truncate(weather.now_hour + 173);
        }
        app.selected_hour = 0;
        app
    }

    /// A window that opens in the evening, so its first day holds only the six
    /// hours left of it.
    ///
    /// The plain fixture opens at midnight, which makes every day a whole one
    /// and hides everything about how the first is navigated — the same trap
    /// the partial *last* day fixture above exists to avoid, at the other end.
    fn app_with_an_evening_start() -> App {
        let mut app = app_with(22, 14);
        if let Fetch::Ready(weather) = &mut app.weather {
            weather.now_hour += 18;
        }
        app.selected_hour = 0;
        app
    }

    #[test]
    fn the_first_day_is_measured_from_the_hour_the_window_opened() {
        assert_eq!(app_with_an_evening_start().hours_left_today(), 6);
        // The plain fixture opens at midnight, so its first day is whole.
        assert_eq!(app_with(22, 14).hours_left_today(), HOURS_PER_DAY);
        // Nothing loaded is not nothing left; the arrows still have to behave.
        assert_eq!(App::new().hours_left_today(), HOURS_PER_DAY);
    }

    /// The complaint. With the window opened at 6 PM, today holds six hours,
    /// and the day arrows step by a whole day at a fixed clock hour — so from
    /// any of the eighteen clock hours today no longer reaches, the cycle
    /// `t, t+24, t+48…` skipped straight over the first day and it could never
    /// be selected. Left and right could still walk onto it an hour at a time,
    /// which is what made it look like the week strip was failing to mark a row
    /// rather than the arrows failing to land on one.
    #[test]
    fn the_day_arrows_can_always_reach_today() {
        let today = app_with_an_evening_start().hours_left_today();

        for start in 0..72usize {
            let mut app = app_with_an_evening_start();
            app.selected_hour = start;

            let mut presses = 0;
            while app.selected_hour >= today {
                app.select_prev_hour_day();
                presses += 1;
                assert!(presses < 12, "start {start} never arrived on today");
            }
        }
    }

    /// Only the step that lands on today may move the clock, and only because
    /// today has no cell at the hour it came from. Every other step keeps it.
    #[test]
    fn only_the_step_onto_today_moves_the_clock() {
        let today = app_with_an_evening_start().hours_left_today();

        for start in today..96usize {
            let mut app = app_with_an_evening_start();
            app.selected_hour = start;
            let before = clock_of(&app);

            app.select_prev_hour_day();

            if start < HOURS_PER_DAY {
                // These are the hours of the second day that today never
                // reaches — before the hour the window opened at — so the step
                // has nowhere on today to preserve the clock and clamps to now.
                assert_eq!(
                    app.selected_hour, 0,
                    "start {start} should have clamped onto now"
                );
            } else {
                // Everything from the second day on steps a whole day back and
                // keeps its hour, including the steps that land on today: hour
                // 25 is 7 PM and so is hour 1.
                assert_eq!(clock_of(&app), before, "start {start} shifted the clock");
            }
        }
    }

    /// Stepping back off today still wraps to the far end, so the arrows keep
    /// their cycle rather than jamming against the first day.
    #[test]
    fn today_still_wraps_to_the_last_day_that_has_the_hour() {
        let mut app = app_with_an_evening_start();
        app.selected_hour = 0;
        app.select_prev_hour_day();

        assert!(
            app.selected_hour >= HOURS_PER_DAY,
            "wrapping should reach the end of the window, not {}",
            app.selected_hour
        );
        assert!(app.selected_hour < app.forecast_hour_stamps().len());
    }

    fn clock_of(app: &App) -> String {
        let stamps = app.forecast_hour_stamps();
        stamps[app.selected_hour][11..].to_string()
    }

    /// The regression. Stepping by whole days must never move the clock, and
    /// wrapping on the raw index did: 7 PM became 2 PM became 9 AM, so the days
    /// it landed on looked arbitrary.
    #[test]
    fn the_day_arrows_never_shift_the_time_of_day() {
        for start in [0usize, 5, 19, 23, 100, 172] {
            let mut app = app_with_a_partial_last_day();
            app.selected_hour = start;
            let expected = clock_of(&app);

            for press in 1..=30 {
                app.select_next_hour_day();
                assert_eq!(
                    clock_of(&app),
                    expected,
                    "start {start}, {press} presses down landed on a different clock hour"
                );
            }
            for press in 1..=30 {
                app.select_prev_hour_day();
                assert_eq!(
                    clock_of(&app),
                    expected,
                    "start {start}, {press} presses up landed on a different clock hour"
                );
            }
        }
    }

    /// Every press has to move somewhere, and a full cycle has to come home —
    /// otherwise the arrows either stick or drift.
    #[test]
    fn a_lap_of_day_steps_visits_each_day_once_and_returns() {
        let mut app = app_with_a_partial_last_day();
        app.selected_hour = 19;

        let mut seen = vec![app.selected_hour];
        loop {
            app.select_next_hour_day();
            if app.selected_hour == 19 {
                break;
            }
            assert!(
                !seen.contains(&app.selected_hour),
                "hour {} came round twice in one lap: {seen:?}",
                app.selected_hour
            );
            seen.push(app.selected_hour);
            assert!(seen.len() < 20, "the lap never closed: {seen:?}");
        }

        // 173 hours from 19:00 reaches that clock hour on seven days.
        assert_eq!(seen, vec![19, 43, 67, 91, 115, 139, 163]);
    }

    /// The last day of the window is usually a partial one, so stepping back
    /// from the first day must reach the last day that actually has this hour,
    /// not simply the end of the series.
    #[test]
    fn stepping_back_from_the_first_day_lands_on_a_real_hour() {
        let mut app = app_with_a_partial_last_day();

        for start in 0..24usize {
            app.selected_hour = start;
            app.select_prev_hour_day();

            let stamps = app.forecast_hour_stamps();
            assert!(
                app.selected_hour < stamps.len(),
                "start {start} left hour {} off the end of {} hours",
                app.selected_hour,
                stamps.len()
            );
            assert_eq!(
                stamps[app.selected_hour][11..],
                stamps[start][11..],
                "start {start} changed the clock"
            );
            assert!(
                app.selected_hour + HOURS_PER_DAY >= stamps.len(),
                "start {start} stopped short of the last day at {}",
                app.selected_hour
            );
        }
    }

    #[test]
    fn the_vertical_arrows_move_a_whole_day() {
        let mut app = app_with(22, 14);

        app.select_next_hour_day();
        assert_eq!(app.selected_hour, 24);

        app.select_prev_hour_day();
        assert_eq!(app.selected_hour, 0);

        // Backwards from now wraps to the same hour on the final day.
        app.select_prev_hour_day();
        assert_eq!(app.selected_hour, 168);
    }

    #[test]
    fn stepping_forward_and_back_returns_to_the_same_hour() {
        let mut app = app_with(22, 14);
        for start in [0, 1, 23, 100, 191] {
            app.selected_hour = start;
            app.select_next_hour();
            app.select_prev_hour();
            assert_eq!(app.selected_hour, start, "round trip from {start}");

            app.select_next_hour_day();
            app.select_prev_hour_day();
            assert_eq!(app.selected_hour, start, "day round trip from {start}");
        }
    }

    #[test]
    fn a_full_lap_of_hours_returns_to_the_start() {
        let mut app = app_with(22, 14);
        for _ in 0..192 {
            app.select_next_hour();
        }
        assert_eq!(app.selected_hour, 0);
    }

    #[test]
    fn now_returns_to_the_current_hour() {
        let mut app = app_with(22, 14);
        app.selected_hour = 137;
        app.select_now();
        assert_eq!(app.selected_hour, 0);
    }

    /// The arrows are live before the first forecast lands, and a location
    /// with no hourly coverage is a real response shape.
    #[test]
    fn hour_navigation_is_inert_without_an_hourly_forecast() {
        let mut app = App::new();
        app.select_next_hour();
        app.select_prev_hour();
        app.select_next_hour_day();
        assert_eq!(app.selected_hour, 0);

        let mut empty = Weather::fixture(3, 1);
        empty.hourly.clear();
        empty.now_hour = 0;
        app.weather = Fetch::Ready(empty);

        app.select_next_hour();
        app.select_prev_hour();
        app.select_prev_hour_day();
        assert_eq!(app.selected_hour, 0);
    }

    /// Whatever the arrows do, the index must stay addressable — a shorter
    /// series after a refresh must not leave the selection off the end.
    #[test]
    fn the_selected_hour_can_never_index_out_of_bounds() {
        let mut app = app_with(22, 14);

        for step in 0..500 {
            match step % 4 {
                0 => app.select_next_hour(),
                1 => app.select_prev_hour(),
                2 => app.select_next_hour_day(),
                _ => app.select_prev_hour_day(),
            }
            let Fetch::Ready(w) = &app.weather else {
                panic!("still ready")
            };
            assert!(
                w.forecast_hours().get(app.selected_hour).is_some(),
                "step {step} left hour {} unaddressable",
                app.selected_hour
            );
        }

        // A refresh that returns a shorter series resets the selection.
        app.selected_hour = 191;
        let mut short = Weather::fixture(3, 1);
        short.hourly.truncate(30);
        short.now_hour = 24;
        let request = app.on_action(Action::Refresh).expect("a refresh request");
        deliver(&mut app, request, short);
        assert_eq!(app.selected_hour, 0, "a new load starts at now");
    }

    fn berlin() -> ActiveLocation {
        ActiveLocation {
            label: "Berlin, Germany".to_string(),
            lat: 52.52437,
            lon: 13.41053,
        }
    }

    #[test]
    fn the_builtin_location_is_new_york_city() {
        assert_eq!(
            ActiveLocation::default(),
            ActiveLocation {
                label: "New York City, New York, United States".to_string(),
                lat: 40.7128,
                lon: -74.0060,
            }
        );
    }

    fn reykjavik() -> ActiveLocation {
        ActiveLocation {
            label: "Reykjavík, Capital Region, Iceland".to_string(),
            lat: 64.146_59,
            lon: -21.942_23,
        }
    }

    fn detecting_app(fallback: ActiveLocation, source: LocationSource) -> App {
        App::with_startup(Startup {
            location: fallback,
            source,
            detect: true,
        })
    }

    fn first_run() -> App {
        detecting_app(ActiveLocation::default(), LocationSource::Fallback)
    }

    /// The promise: a city you picked is not a question the network gets asked
    /// again.
    #[test]
    fn a_chosen_location_skips_detection() {
        let mut app = App::with_location(berlin());

        let Request::Fetch { location, .. } = app.startup_request() else {
            panic!("a chosen city must be fetched, not re-detected")
        };

        assert_eq!(location, berlin());
        assert!(!app.is_locating());
    }

    #[test]
    fn nothing_chosen_detects_first() {
        let mut app = first_run();

        assert!(matches!(app.startup_request(), Request::Detect { .. }));
        assert!(app.is_locating(), "the screen should say what it is doing");
        assert!(matches!(app.weather, Fetch::Loading));
    }

    /// Even with a detection on disk, the point of re-detecting is that it is
    /// yesterday's answer to a question asked again today.
    #[test]
    fn a_previous_detection_is_detected_over() {
        let mut app = detecting_app(berlin(), LocationSource::Detected);

        assert!(matches!(app.startup_request(), Request::Detect { .. }));
    }

    #[test]
    fn an_accepted_detection_fetches_the_detected_place() {
        let mut app = first_run();
        let Request::Detect { id } = app.startup_request() else {
            panic!("not a detection")
        };

        let outcome = app.on_message(Message::Detected {
            id,
            location: reykjavik(),
        });

        assert!(
            outcome.remember.is_none(),
            "a place nobody has seen the weather for is not worth keeping"
        );
        let Some(request) = outcome.request else {
            panic!("a detection must chain a fetch")
        };
        let Request::Fetch { ref location, .. } = request else {
            panic!("not a fetch")
        };
        assert_eq!(*location, reykjavik());
        assert!(!app.is_locating());

        // And the load that answers it is kept as the guess it is.
        assert_eq!(
            deliver(&mut app, request, Weather::fixture(5, 2)),
            Some(Remembered {
                location: reykjavik(),
                source: LocationSource::Detected,
            })
        );
    }

    /// A detection that does not answer costs the user a better guess, not
    /// their forecast.
    #[test]
    fn a_failed_detection_falls_back_without_an_error_screen() {
        let mut app = detecting_app(berlin(), LocationSource::Detected);
        let Request::Detect { id } = app.startup_request() else {
            panic!("not a detection")
        };

        let outcome = app.on_message(Message::DetectFailed {
            id,
            error: "no route to host".to_string(),
        });

        let Some(Request::Fetch { location, .. }) = outcome.request else {
            panic!("a failed detection must still fetch something")
        };
        assert_eq!(
            location,
            berlin(),
            "the remembered detection is the fallback"
        );
        assert!(
            matches!(app.weather, Fetch::Loading),
            "what the user waits on is the fetch, not an error popup"
        );
        assert!(!app.is_locating());
        assert!(
            outcome.warning.unwrap().contains("Berlin"),
            "the reason should name where it landed instead"
        );
    }

    /// The queue is bounded, so a detection can be refused before it is sent.
    /// Left unhandled that is "locating..." forever — worse than the unbounded
    /// growth the bound exists to prevent.
    #[test]
    fn a_refused_detection_falls_back_instead_of_locating_forever() {
        let mut app = first_run();
        let request = app.startup_request();

        let replacement = app.on_dispatch_dropped(request);

        let Some(Request::Fetch { location, .. }) = replacement else {
            panic!("a refused detection must fall back to a fetch")
        };
        assert_eq!(location, ActiveLocation::default());
        assert!(!app.is_locating());
    }

    #[test]
    fn a_stale_detection_is_ignored() {
        let mut app = first_run();
        let _ = app.startup_request();

        let outcome = app.on_message(Message::Detected {
            id: 999,
            location: reykjavik(),
        });

        assert!(
            outcome.request.is_none(),
            "an unasked-for detection chained a fetch"
        );
        assert!(app.is_locating(), "the real detection is still running");
    }

    /// The whole point, end to end: detect, then choose, and the choice is what
    /// survives.
    #[test]
    fn choosing_a_city_after_a_detection_replaces_it_permanently() {
        let mut app = first_run();
        let Request::Detect { id } = app.startup_request() else {
            panic!("not a detection")
        };
        let chained = app
            .on_message(Message::Detected {
                id,
                location: reykjavik(),
            })
            .request
            .expect("a chained fetch");
        deliver(&mut app, chained, Weather::fixture(5, 2));

        app.on_action(Action::OpenSearch);
        app.results = Fetch::Ready(vec![Location {
            name: "Berlin".to_string(),
            admin1: None,
            country: Some("Germany".to_string()),
            lat: 52.52437,
            lon: 13.41053,
        }]);
        let picked = app.on_action(Action::Submit).expect("a fetch for the pick");

        assert_eq!(
            deliver(&mut app, picked, Weather::fixture(5, 2)),
            Some(Remembered {
                location: berlin(),
                source: LocationSource::Chosen,
            })
        );
    }

    #[test]
    fn a_remembered_location_drives_the_initial_fetch() {
        let remembered = berlin();
        let mut app = App::with_location(remembered.clone());

        let Request::Fetch { location, .. } = app.startup_request() else {
            panic!("initial request was not a fetch")
        };

        assert_eq!(app.location, remembered);
        assert_eq!(location, remembered);
    }

    /// Answer a request the way the worker would, so tests exercise the same
    /// correlation the real messages go through.
    fn deliver(app: &mut App, request: Request, weather: Weather) -> Option<Remembered> {
        let Request::Fetch { id, location } = request else {
            panic!("not a fetch")
        };
        app.on_message(Message::Loaded {
            id,
            location,
            weather,
        })
        .remember
    }

    fn id_of(request: &Request) -> RequestId {
        match request {
            Request::Fetch { id, .. } | Request::Search { id, .. } | Request::Detect { id } => *id,
        }
    }

    fn fail(app: &mut App, request: Request) {
        let _ = app.on_message(Message::LoadFailed {
            id: id_of(&request),
            error: "no route to host".to_string(),
        });
    }

    /// Get an app to the state it reaches a moment after launch: the startup
    /// fetch issued and answered. `App::new` begins in `Fetch::Loading`, so a
    /// refresh before that lands is correctly refused.
    fn loaded(app: &mut App) {
        let request = app.startup_request();
        deliver(app, request, Weather::fixture(5, 2));
    }

    /// The bug ActiveLocation exists to kill: `r` used to refetch the
    /// compiled-in default whatever city was on screen, so the user saw
    /// Frederick's weather under Berlin's name.
    #[test]
    fn refresh_follows_the_location_that_loaded() {
        let mut app = App::new();
        let request = app.startup_request();
        deliver(&mut app, request, Weather::fixture(5, 2));

        let request = app.on_action(Action::Refresh).expect("a refresh request");
        let Request::Fetch { location, .. } = &request else {
            panic!("not a fetch")
        };
        assert_eq!(*location, ActiveLocation::default());
        deliver(&mut app, request, Weather::fixture(5, 2));

        // Now switch cities and refresh again.
        let switch = app.fetch(berlin(), LocationSource::Chosen);
        deliver(&mut app, switch, Weather::fixture(5, 2));
        assert_eq!(app.location, berlin(), "the label followed the fetch");

        let Some(Request::Fetch { location, .. }) = app.on_action(Action::Refresh) else {
            panic!("no refresh request")
        };
        assert_eq!(location, berlin(), "refresh went back to the default");
    }

    /// Refresh aims at the request in flight, not the city still on screen —
    /// otherwise a retry after a failed switch quietly reverts your choice.
    #[test]
    fn refresh_retries_the_place_that_was_asked_for() {
        let mut app = App::new();
        loaded(&mut app);

        let switch = app.fetch(berlin(), LocationSource::Chosen);
        fail(&mut app, switch);

        let Some(Request::Fetch { location, .. }) = app.on_action(Action::Refresh) else {
            panic!("no retry request")
        };
        assert_eq!(location, berlin(), "retry keeps chasing Berlin");
    }

    /// Until a fetch succeeds the label must keep describing the measurements
    /// that are actually on screen.
    #[test]
    fn a_failed_switch_never_relabels_the_previous_weather() {
        let mut app = App::new();
        loaded(&mut app);

        let switch = app.fetch(berlin(), LocationSource::Chosen);
        assert!(
            app.on_message(Message::LoadFailed {
                id: id_of(&switch),
                error: "no route to host".to_string(),
            })
            .remember
            .is_none()
        );

        assert_eq!(app.location, ActiveLocation::default());
        assert!(matches!(app.weather, Fetch::Failed(_)));
    }

    #[test]
    fn only_an_accepted_load_reports_a_location_to_persist() {
        let mut app = App::new();
        let stale = app.fetch(ActiveLocation::default(), LocationSource::Chosen);
        let current = app.fetch(berlin(), LocationSource::Chosen);

        assert_eq!(deliver(&mut app, stale, Weather::fixture(5, 2)), None);
        assert_eq!(
            deliver(&mut app, current, Weather::fixture(5, 2)),
            Some(Remembered {
                location: berlin(),
                source: LocationSource::Chosen,
            })
        );
    }

    /// Audit 1.2. Two fetches can be outstanding — press `r`, then pick a
    /// city. The first response must not land at all: committing it would show
    /// the old city's weather, and briefly its name, after you asked for a new
    /// one.
    #[test]
    fn a_weather_response_is_ignored_once_a_newer_request_exists() {
        let mut app = App::new();
        loaded(&mut app);

        let first = app.fetch(ActiveLocation::default(), LocationSource::Chosen);
        let second = app.fetch(berlin(), LocationSource::Chosen);

        deliver(&mut app, first, Weather::fixture(9, 4));
        assert!(
            matches!(app.weather, Fetch::Loading),
            "the stale response should not have resolved the load"
        );

        deliver(&mut app, second, Weather::fixture(5, 2));
        assert_eq!(app.location, berlin());
        assert!(matches!(app.weather, Fetch::Ready(_)));
    }

    /// A stale *failure* must not knock the app into an error state either.
    #[test]
    fn a_stale_failure_never_replaces_a_newer_request() {
        let mut app = App::new();
        let first = app.fetch(ActiveLocation::default(), LocationSource::Chosen);
        let second = app.fetch(berlin(), LocationSource::Chosen);

        fail(&mut app, first);
        assert!(
            matches!(app.weather, Fetch::Loading),
            "the newer request is still running"
        );

        deliver(&mut app, second, Weather::fixture(5, 2));
        assert!(matches!(app.weather, Fetch::Ready(_)));
    }

    /// Audit 1.2, the reported reproduction: search for A, edit the query to
    /// B before A completes, then let A arrive. Pressing Enter afterwards used
    /// to open a result from A.
    #[test]
    fn a_search_response_is_ignored_once_the_query_has_changed() {
        let mut app = App::new();
        app.on_action(Action::OpenSearch);
        for c in "berlin".chars() {
            app.on_action(Action::Insert(c));
        }

        let Some(Request::Search { id, .. }) = app.on_action(Action::Submit) else {
            panic!("no search request")
        };

        // The query moves on while that search is still running.
        app.on_action(Action::Insert('x'));

        let _ = app.on_message(Message::Located {
            id,
            locations: vec![Location {
                name: "Berlin".to_string(),
                admin1: None,
                country: Some("Germany".to_string()),
                lat: 52.52437,
                lon: 13.41053,
            }],
        });

        assert!(
            matches!(app.results, Fetch::Idle),
            "an obsolete query repopulated the list"
        );

        // And Enter now runs the edited query rather than opening a result.
        let Some(Request::Search { query, .. }) = app.on_action(Action::Submit) else {
            panic!("Enter opened a stale result instead of searching")
        };
        assert_eq!(query, "berlinx");
    }

    #[test]
    fn a_stale_search_failure_is_ignored_too() {
        let mut app = App::new();
        app.on_action(Action::OpenSearch);
        app.on_action(Action::Insert('a'));
        let Some(Request::Search { id, .. }) = app.on_action(Action::Submit) else {
            panic!("no search request")
        };

        app.on_action(Action::Insert('b'));
        let _ = app.on_message(Message::SearchFailed {
            id,
            error: "timed out".to_string(),
        });

        assert!(
            matches!(app.results, Fetch::Idle),
            "an obsolete failure surfaced as an error"
        );
    }

    /// An ignored response must leave the current request's own state alone —
    /// not clear its loading indicator, not move the selection.
    #[test]
    fn an_ignored_response_disturbs_nothing() {
        let mut app = App::new();
        loaded(&mut app);
        app.selected_day = 4;
        app.selected_hour = 7;

        let stale = app.fetch(berlin(), LocationSource::Chosen);
        let current = app.fetch(ActiveLocation::default(), LocationSource::Chosen);
        deliver(&mut app, stale, Weather::fixture(9, 1));

        assert!(matches!(app.weather, Fetch::Loading));
        assert_eq!(app.selected_day, 4, "selection moved");
        assert_eq!(app.selected_hour, 7, "hour selection moved");
        drop(current);
    }

    /// Ids must stay distinct however search and weather requests interleave,
    /// or one kind could answer for the other.
    #[test]
    fn request_ids_are_never_reused() {
        let mut app = App::new();
        let mut ids = Vec::new();

        for _ in 0..5 {
            ids.push(id_of(&app.startup_request()));

            app.on_action(Action::OpenSearch);
            app.on_action(Action::Insert('a'));
            if let Some(request) = app.on_action(Action::Submit) {
                ids.push(id_of(&request));
            }
            app.on_action(Action::Back);
        }

        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "an id was reused: {ids:?}");
    }

    /// Audit 2.1. A held `r` against one worker would otherwise queue a fetch
    /// per keypress, and the queue is unbounded.
    #[test]
    fn refresh_is_ignored_while_a_fetch_is_already_running() {
        let mut app = App::new();
        loaded(&mut app);

        assert!(
            app.on_action(Action::Refresh).is_some(),
            "the first is sent"
        );
        for _ in 0..10 {
            assert!(
                app.on_action(Action::Refresh).is_none(),
                "a second fetch was queued behind the first"
            );
        }
    }

    /// The same for leaning on Enter while a search is running.
    #[test]
    fn a_repeated_enter_does_not_queue_duplicate_searches() {
        let mut app = App::new();
        app.on_action(Action::OpenSearch);
        for c in "berlin".chars() {
            app.on_action(Action::Insert(c));
        }

        assert!(app.on_action(Action::Submit).is_some(), "the first is sent");
        for _ in 0..10 {
            assert!(
                app.on_action(Action::Submit).is_none(),
                "a duplicate search was queued"
            );
        }
    }

    /// A failed load has to be retryable, or the app is stuck.
    #[test]
    fn refresh_works_again_once_a_fetch_has_finished() {
        let mut app = App::new();
        let request = app.startup_request();
        fail(&mut app, request);

        assert!(
            app.on_action(Action::Refresh).is_some(),
            "a failed load must be retryable"
        );
    }

    #[test]
    fn quitting_is_recorded_for_the_event_loop() {
        let mut app = App::new();
        assert!(!app.should_quit);
        assert!(app.on_action(Action::Quit).is_none());
        assert!(app.should_quit);
    }

    #[test]
    fn a_search_result_becomes_a_fetchable_location() {
        let found = Location {
            name: "Berlin".to_string(),
            admin1: None,
            country: Some("Germany".to_string()),
            lat: 52.52437,
            lon: 13.41053,
        };

        assert_eq!(ActiveLocation::from(&found), berlin());
    }

    /// Searching is a detour. Whichever screen you left, that is where
    /// choosing a city puts you back — the hourly screen included.
    #[test]
    fn choosing_a_city_returns_to_the_screen_the_search_began_on() {
        for origin in [Screen::Weather, Screen::Hourly] {
            let mut app = app_with(22, 14);
            app.screen = origin;

            app.open_search();
            assert_eq!(app.screen, Screen::Search);

            app.close_search();
            assert_eq!(app.screen, origin, "came back to the wrong screen");
        }
    }

    /// Cancelling has to come back to the same place as choosing, or Esc
    /// becomes its own kind of surprise.
    #[test]
    fn abandoning_the_search_returns_there_too() {
        let mut app = app_with(22, 14);
        app.screen = Screen::Hourly;

        app.open_search();
        app.query.push('b');
        app.close_search();

        assert_eq!(app.screen, Screen::Hourly);
    }

    /// Opening the search twice must not leave the second visit pointing at
    /// the first visit's origin.
    #[test]
    fn the_origin_follows_the_most_recent_search() {
        let mut app = app_with(22, 14);

        app.screen = Screen::Weather;
        app.open_search();
        app.close_search();

        app.screen = Screen::Hourly;
        app.open_search();
        app.close_search();
        assert_eq!(app.screen, Screen::Hourly);
    }

    /// A fresh search starts empty. Leaving results from the last one in place
    /// meant Enter on a blank query reopened a city nobody had searched for.
    #[test]
    fn opening_the_search_discards_the_previous_query_and_its_results() {
        let mut app = app_with(22, 14);
        app.query.push_str("berlin");
        app.results = Fetch::Ready(vec![]);
        app.selected = 2;

        app.open_search();

        assert!(app.query.is_empty());
        assert!(matches!(app.results, Fetch::Idle));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn editing_the_query_discards_results_and_selection() {
        let mut app = App::new();
        app.results = Fetch::Ready(vec![]);
        app.selected = 3;
        app.invalidate_results();
        assert!(matches!(app.results, Fetch::Idle));
        assert_eq!(app.selected, 0);
    }

    /// The failure mode a bounded queue introduces: a request is refused after
    /// the screen has already been moved to `Loading`. Left unhandled that is a
    /// spinner with nothing behind it, which is worse than the unbounded growth
    /// the bound was added to prevent.
    #[test]
    fn a_refused_fetch_reports_failure_instead_of_spinning_forever() {
        let mut app = App::new();
        let request = app.startup_request();
        assert!(matches!(app.weather, Fetch::Loading));

        app.on_dispatch_dropped(request);

        assert!(
            matches!(app.weather, Fetch::Failed(_)),
            "a request that never reached the worker must not leave Loading on screen"
        );
    }

    /// `pending` surviving is what makes the retry aim at the city that failed
    /// rather than the one still on screen — the same rule `LoadFailed` follows.
    #[test]
    fn a_refused_fetch_still_retries_the_place_it_was_aimed_at() {
        let mut app = App::new();
        let target = ActiveLocation {
            label: "Reykjavik, Iceland".to_string(),
            lat: 64.146_59,
            lon: -21.942_23,
        };
        let request = app.fetch(target.clone(), LocationSource::Chosen);
        app.on_dispatch_dropped(request);

        let Some(Request::Fetch { location, .. }) = app.refresh() else {
            panic!("a failed fetch must be retryable");
        };
        assert_eq!(location.label, target.label);
    }

    #[test]
    fn a_refused_search_reports_failure_instead_of_spinning_forever() {
        let mut app = App::new();
        app.query = "reykjavik".to_string();
        let request = app.submit().expect("a non-empty query searches");
        assert!(matches!(app.results, Fetch::Loading));

        app.on_dispatch_dropped(request);

        assert!(matches!(app.results, Fetch::Failed(_)));
        assert!(
            app.submit().is_some(),
            "a refused search must not block the next attempt"
        );
    }

    /// A response to a request that was never sent must not be able to revive
    /// the screen, or the failure above would be undone by a stale arrival.
    #[test]
    fn a_refused_request_ignores_a_late_reply_to_it() {
        let mut app = App::new();
        let request = app.startup_request();
        let Request::Fetch { id, location, .. } = &request else {
            panic!("initial_fetch is a weather fetch");
        };
        let (id, location) = (*id, location.clone());
        app.on_dispatch_dropped(request);

        let _ = app.on_message(Message::Loaded {
            id,
            location,
            weather: Weather::fixture(5, 2),
        });

        assert!(
            matches!(app.weather, Fetch::Ready(_)),
            "the id is still pending, so a genuine reply is still welcome"
        );
    }

    /// The queue has to be deep enough for everything `App` can legitimately
    /// have outstanding, or the bound would refuse ordinary use: one search,
    /// plus a weather fetch the user superseded by picking a new city.
    #[test]
    fn the_queue_is_deeper_than_the_app_can_fill() {
        // From a first run, so the startup request is the detection — the
        // deepest the queue has to go, since the fetch it chains does not exist
        // until the detection has been answered and left the queue.
        let mut app = first_run();
        let mut outstanding = vec![app.startup_request()];

        app.query = "reykjavik".to_string();
        outstanding.push(app.submit().expect("a search runs alongside a fetch"));

        // Every further attempt is declined at the source while its own fetch
        // is still loading, which is the invariant the depth is derived from.
        assert!(app.refresh().is_none(), "refresh is guarded while loading");
        assert!(app.submit().is_none(), "submit is guarded while loading");

        assert!(
            outstanding.len() <= crate::events::REQUEST_QUEUE,
            "{} outstanding requests will not fit in a queue of {}",
            outstanding.len(),
            crate::events::REQUEST_QUEUE
        );
    }

    fn news() -> Message {
        Message::UpdateAvailable {
            notice: "update: virga 9.9.9 is available — run `virga update`".to_string(),
        }
    }

    #[test]
    fn an_update_message_raises_the_notice_and_asks_for_nothing() {
        let mut app = App::new();

        let outcome = app.on_message(news());

        assert!(app.update_notice.is_some());
        assert!(outcome.request.is_none(), "news chains no request");
        assert!(outcome.remember.is_none(), "and persists nothing");
    }

    /// Dismissed by living: the key that clears the notice still does its
    /// job, so the notice can never eat an input.
    #[test]
    fn the_next_action_clears_the_notice_and_still_acts() {
        let mut app = App::new();
        app.on_message(news());

        app.on_action(Action::ToggleUnits);

        assert_eq!(app.update_notice, None);
        assert_eq!(app.unit, Unit::Metric, "the keypress still did its work");
    }

    /// The search screen never renders the notice, so keys pressed there —
    /// every letter of a city name — must not silently delete news nobody
    /// was shown. It stands until a screen that shows it has had a key.
    #[test]
    fn search_keys_do_not_dismiss_a_notice_nobody_saw() {
        let mut app = App::new();
        app.on_action(Action::OpenSearch);
        app.on_message(news());

        app.on_action(Action::Insert('a'));
        assert!(app.update_notice.is_some(), "typing deleted hidden news");

        app.on_action(Action::Back);
        assert!(
            app.update_notice.is_some(),
            "leaving search is the first chance to see it"
        );

        app.on_action(Action::ToggleUnits);
        assert_eq!(app.update_notice, None, "a key that saw it clears it");
    }

    /// A straight-to-quit launch may never give the notice a frame, so quit
    /// leaves it in place for the event loop to hand back to the ordinary
    /// screen — the exit path warnings already use.
    #[test]
    fn quitting_keeps_the_notice_for_the_ordinary_screen() {
        let mut app = App::new();
        app.on_message(news());

        app.on_action(Action::Quit);

        assert!(app.should_quit);
        assert!(app.update_notice.is_some());
    }
}
