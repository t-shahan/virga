use crate::app::ActiveLocation;
use crate::cache;
use crate::weather::client::detect_location;
use crate::weather::client::fetch_forecast;
use crate::weather::client::search_locations;
use crate::weather::model::Location;
use crate::weather::model::Weather;
use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, SyncSender, TrySendError};
use std::thread;

/// Correlates a request with the message that answers it. The worker's
/// processing order is not an identity guarantee: two requests can be in
/// flight, and the app has to be able to tell which one came back.
pub type RequestId = u64;

/// How many requests may sit unclaimed in one worker's queue before it
/// refuses more.
///
/// `App` already declines to queue a duplicate — `refresh` and `submit` both
/// bail while their fetch is `Loading` — so the reachable depth of any one
/// queue is a request plus the one the user superseded, by picking a new
/// city or by editing the query. Two slots is that invariant; four is
/// headroom for it being wrong.
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
    /// A forecast reached the app but its copy for the next launch did not
    /// reach the disk. No id: the forecast it concerns is already on
    /// screen, and nothing chains off it.
    CacheFailed {
        error: String,
    },
}

/// One release probe on its own one-shot thread, never one of the request
/// queues. Each queue serves its kind in order, so a probe put on the
/// search queue would stall a city search behind a slow answer from GitHub,
/// and the other two are no better a fit for a one-shot with no id and
/// nothing chaining off it. Sends at most one message and ends.
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

/// The request queues, one per kind of request, behind one handle.
///
/// One queue and one thread used to serve everything in arrival order, and
/// the forecast's fifteen-second timeout was the price of that: a city
/// search typed while a fetch was stalled sat behind it, spinner and all,
/// with nothing to say. Each kind now has a thread of its own, so the only
/// request a search can wait behind is another search. The kinds are
/// independent by construction — `App` matches every answer to the request
/// it is waiting on by id — so nothing relied on the old order except the
/// detection race `App::fetch` now closes for itself.
pub struct Workers {
    fetch: SyncSender<Request>,
    detect: SyncSender<Request>,
    search: SyncSender<Request>,
}

impl Workers {
    /// Queue a request on its kind's worker without waiting. The bound is
    /// per queue, so the error carries the request back exactly as one
    /// channel's would, and the caller's handling need not know how many
    /// queues there are.
    pub fn try_send(&self, request: Request) -> Result<(), TrySendError<Request>> {
        match request {
            Request::Fetch { .. } => self.fetch.try_send(request),
            Request::Detect { .. } => self.detect.try_send(request),
            Request::Search { .. } => self.search.try_send(request),
        }
    }
}

/// `cache` is where a fetched forecast is kept for the next launch, or
/// `None` to keep nothing.
pub fn spawn_workers(messages: Sender<Message>, cache: Option<PathBuf>) -> Workers {
    spawn_workers_with(messages, cache, serve)
}

/// The workers with the network swapped out. `serve` answers each request;
/// `main` passes the real one, and a test passes one that can be told to
/// stall, which is how "a search is answered while a fetch is stalled" is
/// proved rather than timed.
fn spawn_workers_with(
    messages: Sender<Message>,
    cache: Option<PathBuf>,
    serve: impl Fn(Request) -> Message + Send + Sync + 'static,
) -> Workers {
    let serve = Arc::new(serve);
    let spawn = |cache: Option<PathBuf>| {
        let (tx, rx) = mpsc::sync_channel(REQUEST_QUEUE);
        spawn_worker(rx, messages.clone(), cache, Arc::clone(&serve));
        tx
    };
    // Only a fetch produces a forecast, so only its worker is told where to
    // keep one; the others would never write.
    Workers {
        fetch: spawn(cache),
        detect: spawn(None),
        search: spawn(None),
    }
}

fn serve(request: Request) -> Message {
    match request {
        Request::Fetch { id, location } => match fetch_forecast(location.lat, location.lon) {
            Ok(weather) => Message::Loaded {
                id,
                location,
                weather,
            },
            Err(e) => Message::LoadFailed {
                id,
                error: e.to_string(),
            },
        },
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
    }
}

/// One thread serving one queue in order. The cache write happens here, on
/// the thread that already waits on the network, and only after the forecast
/// has been sent: the app must never sit behind an fsync for its frame.
fn spawn_worker(
    requests: Receiver<Request>,
    messages: Sender<Message>,
    cache: Option<PathBuf>,
    serve: Arc<impl Fn(Request) -> Message + Send + Sync + 'static>,
) {
    thread::spawn(move || {
        for request in requests {
            let message = serve(request);
            // Encoded before the forecast is handed over, because handing it
            // over moves it.
            let keep = match (&cache, &message) {
                (
                    Some(path),
                    Message::Loaded {
                        location, weather, ..
                    },
                ) => Some((path.clone(), cache::encode(location, weather, Utc::now()))),
                _ => None,
            };
            if messages.send(message).is_err() {
                break;
            }
            if let Some((path, encoded)) = keep
                && let Err(error) = encoded.and_then(|bytes| cache::write(&path, &bytes))
                && messages
                    .send(Message::CacheFailed {
                        error: format!("{error:#}"),
                    })
                    .is_err()
            {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    const PATIENCE: Duration = Duration::from_secs(5);

    /// Workers whose fetches block until `release` is signalled, once per
    /// fetch, and whose other requests answer at once. A blocking channel
    /// rather than a slow server: the stall is under the test's control
    /// and lasts exactly as long as the test says.
    fn stalling_workers(messages: Sender<Message>) -> (Workers, Sender<()>) {
        let (release, released) = mpsc::channel::<()>();
        let released = Mutex::new(released);
        let workers = spawn_workers_with(messages, None, move |request| match request {
            Request::Fetch { id, .. } => {
                // A dropped sender ends the stall too, so a failing test
                // finishes rather than hangs.
                let _ = released.lock().unwrap().recv();
                Message::LoadFailed {
                    id,
                    error: "released".to_string(),
                }
            }
            Request::Detect { id } => Message::Detected {
                id,
                location: ActiveLocation::default(),
            },
            Request::Search { id, .. } => Message::Located {
                id,
                locations: Vec::new(),
            },
        });
        (workers, release)
    }

    fn fetch(id: RequestId) -> Request {
        Request::Fetch {
            id,
            location: ActiveLocation::default(),
        }
    }

    fn search(id: RequestId) -> Request {
        Request::Search {
            id,
            query: "reykjavik".to_string(),
        }
    }

    /// The complaint: pressing `l`, typing a city and hitting Enter while a
    /// fetch was stalled showed the spinner for the whole of the fetch's
    /// timeout, because the search sat behind it in the one queue. The
    /// search, and a detection, must answer while the fetch is still held.
    #[test]
    fn a_search_and_a_detection_are_answered_while_a_fetch_is_stalled() {
        let (tx, rx) = mpsc::channel();
        let (workers, release) = stalling_workers(tx);

        workers.try_send(fetch(1)).expect("room for a fetch");
        workers.try_send(search(2)).expect("room for a search");
        workers
            .try_send(Request::Detect { id: 3 })
            .expect("room for a detection");

        // Nothing can answer the fetch until it is released, so the first
        // two answers are the other two kinds, in whichever order.
        let mut answered = Vec::new();
        for _ in 0..2 {
            match rx.recv_timeout(PATIENCE) {
                Ok(Message::Located { id, .. }) | Ok(Message::Detected { id, .. }) => {
                    answered.push(id);
                }
                Ok(_) => panic!("the stalled fetch answered first"),
                Err(e) => panic!("an answer sat behind the stalled fetch: {e}"),
            }
        }
        answered.sort_unstable();
        assert_eq!(answered, vec![2, 3]);

        release.send(()).expect("the fetch worker is waiting");
        assert!(
            matches!(
                rx.recv_timeout(PATIENCE),
                Ok(Message::LoadFailed { id: 1, .. })
            ),
            "the fetch answers once released"
        );
    }

    /// Each kind has its own bound. Filling one queue must refuse only that
    /// kind — the request handed back is the one that did not fit, which is
    /// what `on_dispatch_dropped` needs — and leave the others open.
    #[test]
    fn a_full_queue_refuses_its_own_kind_and_no_other() {
        let (tx, rx) = mpsc::channel();
        let (workers, release) = stalling_workers(tx);

        // The first fetch is held on the gate and the rest pile up behind
        // it. Whether the worker has taken that first one off the queue yet
        // is not the test's to know, so it pushes until refused rather than
        // counting to the bound.
        let mut id = 0;
        let refused = loop {
            match workers.try_send(fetch(id)) {
                Ok(()) => id += 1,
                Err(TrySendError::Full(Request::Fetch { id: back, .. })) => break back,
                Err(e) => panic!("a full queue must hand the request back: {e}"),
            }
        };
        assert_eq!(refused, id, "the request handed back is the one refused");
        assert!(
            id >= REQUEST_QUEUE as RequestId,
            "refused after {id} fetches, which is fewer than the bound"
        );

        workers
            .try_send(search(7))
            .expect("a full fetch queue must not refuse a search");
        assert!(matches!(
            rx.recv_timeout(PATIENCE),
            Ok(Message::Located { id: 7, .. })
        ));
        drop(release);
    }

    /// A worker sends nothing after the app has stopped listening, and a
    /// send into a worker that has gone is reported, not swallowed.
    #[test]
    fn workers_end_with_the_message_channel() {
        let (tx, rx) = mpsc::channel();
        let workers = spawn_workers_with(tx, None, |request| match request {
            Request::Fetch { id, .. } | Request::Detect { id } | Request::Search { id, .. } => {
                Message::SearchFailed {
                    id,
                    error: "unused".to_string(),
                }
            }
        });
        workers.try_send(search(1)).expect("room for a search");
        assert!(matches!(
            rx.recv_timeout(PATIENCE),
            Ok(Message::SearchFailed { id: 1, .. })
        ));

        drop(rx);
        // The worker only learns the receiver is gone when it next sends, so
        // requests are accepted, and may even pile up, until it has served
        // one more. Every one of them is refused once it has. The deadline
        // is a safety bound, not a timing assertion: a worker that never
        // notices fails here with a message instead of hanging the run.
        let deadline = Instant::now() + PATIENCE;
        loop {
            match workers.try_send(search(2)) {
                Err(TrySendError::Disconnected(_)) => break,
                Ok(()) | Err(TrySendError::Full(_)) => {
                    assert!(
                        Instant::now() < deadline,
                        "the worker kept accepting requests after the app stopped listening"
                    );
                    thread::yield_now();
                }
            }
        }
    }

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
