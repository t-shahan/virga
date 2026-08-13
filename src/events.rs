use crate::app::ActiveLocation;
use crate::weather::client::detect_location;
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

/// How many requests may sit unclaimed before the channel refuses more.
///
/// `App` already declines to queue a duplicate — `refresh` and `submit` both
/// bail while their fetch is `Loading` — so the reachable depth is one search
/// plus a weather fetch the user superseded by picking a new city. Two slots
/// is that invariant; four is headroom for it being wrong.
///
/// The bound is the point. The guards in `App` are a promise made in prose,
/// and prose does not survive a refactor: drop one and an unbounded channel
/// would absorb the mistake silently, growing without limit behind a worker
/// that handles one request at a time. A bounded channel makes that same
/// mistake fail loudly and immediately instead.
pub const REQUEST_QUEUE: usize = 4;

pub enum Request {
    Fetch {
        id: RequestId,
        location: ActiveLocation,
    },
    /// Ask the network where the caller is. Startup only, and at most once a
    /// launch — it carries no coordinates because working them out is the
    /// entire job.
    Detect {
        id: RequestId,
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
    /// Where the caller turned out to be. Not a forecast and not yet on screen:
    /// the app answers it with a fetch.
    Detected {
        id: RequestId,
        location: ActiveLocation,
    },
    DetectFailed {
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
                Request::Detect { id } => match detect_location() {
                    Ok(found) => Message::Detected {
                        id,
                        location: ActiveLocation::from(&found),
                    },
                    Err(e) => Message::DetectFailed {
                        id,
                        error: e.to_string(),
                    },
                },
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
