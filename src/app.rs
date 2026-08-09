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
            location: None,
        }
    }
}
