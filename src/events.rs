use crate::app::ActiveLocation;
use crate::weather::client::fetch_forecast;
use crate::weather::client::search_locations;
use crate::weather::model::Location;
use crate::weather::model::Weather;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

pub enum Request {
    Fetch(ActiveLocation),
    Search(String),
}

pub enum Message {
    /// Carries the place it was fetched for. Without it the app would have to
    /// assume a response answers the newest request, which is false the moment
    /// two are in flight.
    Loaded(ActiveLocation, Weather),
    LoadFailed(String),
    SearchFailed(String),
    Located(Vec<Location>),
}

pub fn spawn_worker(requests: Receiver<Request>, messages: Sender<Message>) {
    thread::spawn(move || {
        for request in requests {
            let message = match request {
                Request::Fetch(location) => match fetch_forecast(location.lat, location.lon) {
                    Ok(weather) => Message::Loaded(location, weather),
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
