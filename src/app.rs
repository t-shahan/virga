use crate::units::Unit;
use crate::weather::model::{Location, Weather};

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
    Precipitation,
}

/// How far the vertical arrows jump. Eight days is a long way at one press
/// per hour.
const HOURS_PER_DAY: usize = 24;

/// A place the app can fetch for, with its label and coordinates in one value.
/// They were separate before: selecting a search result stored the label and
/// discarded the coordinates, so refresh silently fell back to the default city
/// while the border kept showing the chosen one.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveLocation {
    pub label: String,
    pub lat: f64,
    pub lon: f64,
}

impl Default for ActiveLocation {
    fn default() -> Self {
        Self {
            label: "Frederick, Maryland, United States".to_string(),
            lat: 39.414_27,
            lon: -77.410_54,
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

pub struct App {
    pub screen: Screen,
    pub query: String,
    pub results: Fetch<Vec<Location>>,
    pub weather: Fetch<Weather>,
    pub unit: Unit,
    pub tick: usize,
    pub selected: usize,
    /// Index into `Weather::daily` of the day being inspected. Distinct from
    /// `selected`, which tracks the search results list.
    pub selected_day: usize,
    /// Index into `Weather::forecast_hours()` — the hourly series from now
    /// onward, so zero is always the current hour.
    pub selected_hour: usize,
    /// The place the displayed weather actually describes. Only a successful
    /// load moves it, so the label can never get ahead of the measurements.
    pub location: ActiveLocation,
    /// A place we have asked for but not yet heard back about. Refresh and
    /// retry aim here, so a failed switch retries the city you asked for
    /// rather than the one still on screen.
    pub pending: Option<ActiveLocation>,
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Weather,
            query: String::new(),
            results: Fetch::Idle,
            weather: Fetch::Loading,
            unit: Unit::Imperial,
            tick: 0,
            selected: 0,
            selected_day: 0,
            selected_hour: 0,
            location: ActiveLocation::default(),
            pending: None,
        }
    }

    /// What `r` should fetch: whatever we are already chasing, else what is on
    /// screen. Never the compiled-in default — that was the bug.
    pub fn refresh_target(&self) -> ActiveLocation {
        self.pending
            .clone()
            .unwrap_or_else(|| self.location.clone())
    }

    /// Aim at a new place. The label does not move yet; `commit` does that.
    pub fn request(&mut self, location: ActiveLocation) -> ActiveLocation {
        self.pending = Some(location.clone());
        self.weather = Fetch::Loading;
        location
    }

    /// A load arrived. Adopt the location it was for, rather than assuming it
    /// answers the most recent request — two fetches can be in flight at once.
    pub fn commit(&mut self, location: ActiveLocation, weather: Weather) {
        if self.pending.as_ref() == Some(&location) {
            self.pending = None;
        }
        self.selected_day = weather.today_index;
        self.selected_hour = 0;
        self.location = location;
        self.weather = Fetch::Ready(weather);
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
        } else {
            // The last day that reaches this time of day. The final day of the
            // window is usually partial, so that is not always the last day.
            let days = (count - 1 - time_of_day) / HOURS_PER_DAY;
            time_of_day + days * HOURS_PER_DAY
        };
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

    /// Results only describe the query that produced them, so any edit to the
    /// query discards them. Without this, Enter keeps taking the "select"
    /// branch and opens a city from the previous search instead of running a
    /// new one.
    pub fn invalidate_results(&mut self) {
        self.results = Fetch::Idle;
        self.selected = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app_with(days: usize, today: usize) -> App {
        let mut app = App::new();
        app.weather = Fetch::Ready(Weather::fixture(days, today));
        app.selected_day = today;
        app
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
        app.commit(ActiveLocation::default(), short);
        assert_eq!(app.selected_hour, 0, "a new load starts at now");
    }

    fn berlin() -> ActiveLocation {
        ActiveLocation {
            label: "Berlin, Germany".to_string(),
            lat: 52.52437,
            lon: 13.41053,
        }
    }

    /// The bug this type exists to kill: `r` used to refetch the compiled-in
    /// default whatever city was on screen, so the user saw Frederick's weather
    /// under Berlin's name.
    #[test]
    fn refresh_follows_the_location_that_loaded() {
        let mut app = App::new();
        assert_eq!(app.refresh_target(), ActiveLocation::default());

        app.request(berlin());
        app.commit(berlin(), Weather::fixture(5, 2));

        assert_eq!(app.refresh_target(), berlin());
        assert_eq!(app.location, berlin(), "the label followed the fetch");
    }

    /// Refresh aims at the request in flight, not the city still on screen —
    /// otherwise a retry after a failed switch quietly reverts your choice.
    #[test]
    fn refresh_retries_the_place_that_was_asked_for() {
        let mut app = App::new();
        app.commit(ActiveLocation::default(), Weather::fixture(5, 2));

        app.request(berlin());
        app.weather = Fetch::Failed("timed out".to_string());

        assert_eq!(app.refresh_target(), berlin(), "retry keeps chasing Berlin");
    }

    /// Until a fetch succeeds the label must keep describing the measurements
    /// that are actually on screen.
    #[test]
    fn a_failed_switch_never_relabels_the_previous_weather() {
        let mut app = App::new();
        app.commit(ActiveLocation::default(), Weather::fixture(5, 2));

        app.request(berlin());
        app.weather = Fetch::Failed("no route to host".to_string());

        assert_eq!(app.location, ActiveLocation::default());
    }

    /// Two fetches can be in flight at once — press `r`, then pick a city. The
    /// response carries its own location, so the first cannot commit under the
    /// second's name.
    #[test]
    fn a_response_commits_its_own_location_not_the_newest_request() {
        let mut app = App::new();
        app.request(ActiveLocation::default());
        app.request(berlin());

        app.commit(ActiveLocation::default(), Weather::fixture(5, 2));

        assert_eq!(app.location, ActiveLocation::default());
        assert_eq!(app.pending, Some(berlin()), "Berlin is still outstanding");
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

    #[test]
    fn editing_the_query_discards_results_and_selection() {
        let mut app = App::new();
        app.results = Fetch::Ready(vec![]);
        app.selected = 3;
        app.invalidate_results();
        assert!(matches!(app.results, Fetch::Idle));
        assert_eq!(app.selected, 0);
    }
}
