use crate::app::ActiveLocation;
use crate::weather::client::fetch_forecast;
use crate::weather::client::search_locations;
use crate::weather::model::Location;
use crate::weather::model::Weather;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;

/// Correlates a request with the message that answers it. The worker's
/// processing order is not an identity guarantee: two requests can be in
/// flight, and the app has to be able to tell which one came back.
pub type RequestId = u64;

pub enum Request {
    Fetch {
        id: RequestId,
        location: ActiveLocation,
    },
    Search {
        id: RequestId,
        query: String,
    },
}

pub enum Message {
    /// Carries the place it was fetched for as well as its id. Without the
    /// location the app would have to assume a response answers the newest
    /// request, which is false the moment two are outstanding.
    Loaded {
        id: RequestId,
        location: ActiveLocation,
        weather: Weather,
    },
    LoadFailed {
        id: RequestId,
        error: String,
    },
    Located {
        id: RequestId,
        locations: Vec<Location>,
    },
    SearchFailed {
        id: RequestId,
        error: String,
    },
}

pub fn spawn_worker(requests: Receiver<Request>, messages: Sender<Message>) {
    thread::spawn(move || {
        for request in requests {
            let message = match request {
                Request::Fetch { id, location } => {
                    match fetch_forecast(location.lat, location.lon) {
                        Ok(weather) => Message::Loaded {
                            id,
                            location,
                            weather,
                        },
                        Err(e) => Message::LoadFailed {
                            id,
                            error: e.to_string(),
                        },
                    }
                }
                Request::Search { id, query } => match search_locations(&query) {
                    Ok(locations) => Message::Located { id, locations },
                    Err(e) => Message::SearchFailed {
                        id,
                        error: e.to_string(),
                    },
                },
            };
            if messages.send(message).is_err() {
                break;
            }
        }
    });
}
