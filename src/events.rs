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
    /// A newer release exists. Carries the finished notice text, composed in
    /// the probe, so the app holds one string and never learns about paths,
    /// versions, or the network. No id: nothing chains off it, nothing
    /// supersedes it, and at most one is ever sent.
    UpdateAvailable {
        notice: String,
    },
}

/// One release probe on its own one-shot thread — never the request queue,
/// where the worker serves requests serially and a slow answer from GitHub
/// would stall a city search behind it. Sends at most one message and ends.
///
/// The probe is injected so a test never opens a socket; `main` passes the
/// real one. A probe with nothing to say returns `None`, and failure *is*
/// nothing to say — the weather fetch complains about the network when the
/// network deserves complaining about.
pub fn spawn_update_check(
    messages: Sender<Message>,
    probe: impl FnOnce() -> Option<String> + Send + 'static,
) {
    thread::spawn(move || {
        if let Some(notice) = probe() {
            // A send after the app has quit is a dropped receiver, and
            // ignoring that error is the whole shutdown story.
            let _ = messages.send(Message::UpdateAvailable { notice });
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    /// The check thread's whole contract: at most one message, and it ends
    /// either way. Once the probe's sender is dropped the receiver reports
    /// disconnection, which is how "sent nothing" is proved rather than
    /// merely waited on.
    #[test]
    fn a_probe_with_news_sends_one_update_message() {
        let (tx, rx) = mpsc::channel();

        spawn_update_check(tx, || Some("update: virga 9.9.9 is available".to_string()));

        let Ok(Message::UpdateAvailable { notice }) = rx.recv_timeout(Duration::from_secs(5))
        else {
            panic!("the probe's news never arrived");
        };
        assert!(notice.contains("9.9.9"));
        assert!(
            rx.recv_timeout(Duration::from_secs(5)).is_err(),
            "one probe must not send twice"
        );
    }

    #[test]
    fn a_probe_with_nothing_to_say_sends_nothing() {
        let (tx, rx) = mpsc::channel();

        spawn_update_check(tx, || None);

        assert!(
            matches!(
                rx.recv_timeout(Duration::from_secs(5)),
                Err(mpsc::RecvTimeoutError::Disconnected)
            ),
            "the thread should end without sending, not linger"
        );
    }

    /// The app quitting drops the receiver; a probe answering afterwards must
    /// die quietly rather than panic the detached thread.
    #[test]
    fn an_answer_after_quit_is_dropped_without_complaint() {
        let (tx, rx) = mpsc::channel::<Message>();
        drop(rx);

        spawn_update_check(tx, || Some("too late".to_string()));
        // Nothing to assert beyond "no panic": the thread is detached, and
        // the send error is swallowed by design.
    }
}
