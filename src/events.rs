use crate::Location;
use crate::weather::client::fetch_forecast;
use crate::weather::client::search_locations;
use crate::weather::model::Weather;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub enum Request {
    Fetch { lat: f64, lon: f64 },
    Search(String),
}

pub enum Message {
    Loaded(Weather),
    LoadFailed(String),
    SearchFailed(String),
    Located(Vec<Location>),
}

pub fn spawn_worker(requests: Receiver<Request>, messages: Sender<Message>) {
    thread::spawn(move || {
        for request in requests {
            let message = match request {
                Request::Fetch { lat, lon } => match fetch_forecast(lat, lon) {
                    Ok(weather) => Message::Loaded(weather),
                    Err(e) => Message::LoadFailed(e.to_string()),
                },
                Request::Search(query) => match search_locations(&query) {
                    Ok(locations) => Message::Located(locations),
                    Err(e) => Message::SearchFailed(e.to_string()),
                },
            };
            if messages.send(message).is_err() {
                break;
            }
        }
    });
}
