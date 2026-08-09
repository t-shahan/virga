use crate::units::Unit;
use crate::weather::model::{Location, Weather};

pub enum Fetch<T> {
    Idle,
    Loading,
    Ready(T),
    Failed(String),
}

pub enum Screen {
    Weather,
    Search,
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
    pub location: Option<String>,
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
            location: None,
        }
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
