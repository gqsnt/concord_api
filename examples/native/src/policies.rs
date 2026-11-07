//! U30 / U31 (R10–R12): application-owned policies composed around Concord.
//!
//! `Prepared::send` and generated `send()` without a client send policy perform one
//! `reqwest::Client::execute` (R11), with no Concord retry or extra request. A client
//! `send policy;` instead delegates to `SendWith<T>::send`, which may sign, make sub-requests or
//! repeat requests. Each recipe below is ordinary application code with its assumptions stated;
//! none is a recommended default for arbitrary APIs.

use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use concord::ErrorKind;
use concord_ratelimit::{Limiter, Mode, TokioClock};
use std::num::NonZeroUsize;

use crate::api::{Inventory, Item};

/// Items collected by [`collect_items`].
#[derive(Debug, Clone)]
pub struct Collected {
    /// Items from every page fetched, in order.
    pub items: Vec<Item>,
    /// The cursor of the first page **not** fetched because `max_pages` was reached; `None`
    /// when the listing ended.
    pub next: Option<String>,
}

/// Walks the cursor-paginated listing, fetching at most `max_pages` pages (U30).
///
/// The bound and the stopping rule are the application's: a server that keeps returning a
/// cursor cannot make this loop run forever, and nothing is silently lost — the unused cursor
/// is returned. Each page is a fresh request from a cloned template (`limit` stays as declared,
/// `maybe_cursor` sets or clears the cursor). A failed page returns its error; pages already
/// fetched are dropped with it, so callers that need them should page manually.
pub async fn collect_items(api: &Inventory, max_pages: usize) -> Result<Collected, concord::Error> {
    let template = api.list();
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..max_pages {
        let page = template.clone().maybe_cursor(cursor.take()).send().await?;
        items.extend(page.items);
        match page.next {
            Some(next) => cursor = Some(next),
            None => return Ok(Collected { items, next: None }),
        }
    }
    Ok(Collected {
        items,
        next: cursor,
    })
}

/// Runs `attempt` up to `max_attempts` times (at least once), sleeping `backoff * n` after the
/// n-th failure, while `retryable` accepts the error.
///
/// `attempt` is a request **factory**: it builds a new request each time. Concord's `Clone` on a
/// request (when its values are `Clone`) only means it can be copied; it says nothing about
/// whether re-sending is safe. Deciding that an operation may be replayed, and which failures
/// justify it, is entirely the caller's responsibility.
pub async fn retry_bounded<T, F, Fut>(
    max_attempts: usize,
    backoff: Duration,
    mut attempt: F,
    retryable: impl Fn(&concord::Error) -> bool,
) -> Result<T, concord::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, concord::Error>>,
{
    use concord_retry::{Decision, Policy, Replay, Retry};
    use std::num::NonZeroUsize;

    let policy = Policy {
        initial_delay: backoff,
        max_delay: Duration::MAX,
        ..Policy::new(NonZeroUsize::new(max_attempts.max(1)).expect("at least once"))
    };
    Retry::new(policy, Replay::Permitted)
        .run_tokio(
            |_| attempt(),
            |result| match result {
                Err(error) if retryable(error) => Decision::RetryAfter(None),
                _ => Decision::Done,
            },
            // Preserve this recipe's linear schedule within the engine's exponential ceiling.
            |n, _| backoff.saturating_mul(u32::try_from(n).unwrap_or(u32::MAX)),
        )
        .await
        .outcome
        .expect("a nonzero attempt budget without a deadline always completes an attempt")
}

/// Fetches an item, retrying a bounded number of times when the connection failed (U30).
///
/// Assumptions made **by this application**, not by Concord: `GET items/{id}` is read-only, so
/// replaying it has no effect on the server; and only failures where no response arrived
/// (`ErrorKind::Transport` with `is_connect()` or `is_request()`, but not `is_timeout()`) are
/// worth retrying — a timeout may mean the server is overloaded, and a status or decode error
/// would just repeat. For an operation with side effects do not reuse this predicate: whether a
/// write may be replayed depends on the API (idempotency keys, conditional requests).
pub async fn item_with_retry(
    api: &Inventory,
    id: u64,
    max_attempts: usize,
) -> Result<Item, concord::Error> {
    retry_bounded(
        max_attempts,
        Duration::from_millis(10),
        || api.item(id).send(),
        |error| {
            error.kind() == ErrorKind::Transport
                && error
                    .transport()
                    .is_some_and(|e| (e.is_connect() || e.is_request()) && !e.is_timeout())
        },
    )
    .await
}

/// Application-owned shared concurrency admission for inventory requests.
pub struct Admission(Limiter<()>);
impl Admission {
    /// Create admission with a nonzero concurrency limit.
    pub fn new(limit: NonZeroUsize) -> Self {
        let limiter = Limiter::new(NonZeroUsize::new(1).unwrap(), TokioClock::default());
        limiter.set_concurrency((), limit).unwrap();
        Self(limiter)
    }
    /// Number of currently unused permits.
    pub fn available_permits(&self) -> usize {
        self.0.available_leases(&()).unwrap()
    }
}

/// Fetch items with bounded concurrency, holding a lease until each body is decoded.
/// Requests are built before admission; results remain in input order.
pub async fn items_admitted(
    api: &Inventory,
    ids: &[u64],
    admission: Arc<Admission>,
) -> Vec<Result<Item, concord::Error>> {
    let mut tasks = Vec::with_capacity(ids.len());
    for &id in ids {
        let prepared = api.item(id).prepare();
        let admission = Arc::clone(&admission);
        tasks.push(tokio::spawn(async move {
            let prepared = prepared?;
            let _permit = admission
                .0
                .lease((), Mode::Wait)
                .await
                .expect("the configured key is retained");
            prepared.send().await
        }));
    }
    let mut results = Vec::with_capacity(tasks.len());
    for task in tasks {
        results.push(task.await.expect("request task panicked"));
    }
    results
}

/// The application's holder of the current [`Inventory`] handle and its authorization (U31).
///
/// Concord stores the `key` it is given and never fetches or refreshes it. When the
/// application obtains a new value (by whatever means it uses), [`Session::set_key`] swaps in a
/// handle made with the client's generated `set_key(..)`; handles, scopes and requests created
/// earlier keep the key they captured (snapshot semantics), so an in-flight or queued request is
/// never altered behind its back.
#[derive(Debug)]
pub struct Session {
    current: RwLock<Inventory>,
}

impl Session {
    /// Wraps a handle.
    pub fn new(api: Inventory) -> Self {
        Self {
            current: RwLock::new(api),
        }
    }

    /// The current handle (a cheap `Arc` clone).
    pub fn api(&self) -> Inventory {
        self.current
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Replaces the authorization value for requests created from now on.
    pub fn set_key(&self, key: &str) {
        let mut current = self
            .current
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = current.clone().set_key(key);
    }
}
