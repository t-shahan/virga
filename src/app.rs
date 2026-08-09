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

    pub fn select_prev_day(&mut self) {
        self.selected_day = self.selected_day.saturating_sub(1);
    }

    pub fn select_next_day(&mut self) {
        if let Fetch::Ready(weather) = &self.weather
            && self.selected_day + 1 < weather.daily.len()
        {
            self.selected_day += 1;
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
