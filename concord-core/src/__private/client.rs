use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use crate::base::{Base, BaseError, IntoBase};
use crate::middleware::{ErasedMiddleware, ErasedTransport, Middleware, Next};
use crate::retry::Retry;
use crate::transport::Transport;

type DefaultTransport = std::sync::OnceLock<Result<crate::Http, Arc<reqwest::Error>>>;

/// The state of a client: its middlewares, what sends, the retry policy, the two clocks,
/// and a base that replaces the description's.
pub struct Core<P> {
    retry: Option<Retry<P>>,
    timeout: Option<Duration>,
    idle_timeout: Option<Duration>,
    middlewares: Vec<Arc<dyn ErasedMiddleware>>,
    caller_middlewares: usize,
    base: Option<Result<Base, BaseError>>,
    // None until a transport is chosen; the engine supplies its default Http when sending.
    transport: Option<Arc<dyn ErasedTransport>>,
    default_transport: Arc<DefaultTransport>,
}

impl<P> fmt::Debug for Core<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Core").finish_non_exhaustive()
    }
}

impl<P> Clone for Core<P> {
    fn clone(&self) -> Core<P> {
        Core {
            retry: self.retry.clone(),
            timeout: self.timeout,
            idle_timeout: self.idle_timeout,
            middlewares: self.middlewares.clone(),
            caller_middlewares: self.caller_middlewares,
            base: self.base.clone(),
            transport: self.transport.clone(),
            default_transport: self.default_transport.clone(),
        }
    }
}

impl<P> Default for Core<P> {
    fn default() -> Core<P> {
        Core {
            retry: None,
            timeout: None,
            idle_timeout: None,
            middlewares: Vec::new(),
            caller_middlewares: 0,
            base: None,
            transport: None,
            default_transport: Arc::new(std::sync::OnceLock::new()),
        }
    }
}

impl<P> Core<P> {
    /// `with_transport` of a client: replaces only what really sends, under its middlewares.
    pub fn with_transport(mut self, transport: impl Transport) -> Core<P> {
        self.transport = Some(Arc::new(transport));
        self
    }

    /// Adds a description's middleware in `new`, in the order written, made once and shared.
    #[doc(hidden)]
    pub fn add_middleware(mut self, middleware: impl Middleware) -> Core<P> {
        self.middlewares.push(Arc::new(middleware));
        self
    }

    /// `with_middleware` of a client: adds before the description's, in caller order.
    pub fn with_middleware(mut self, middleware: impl Middleware) -> Core<P> {
        self.middlewares
            // PANIC-SAFETY: The caller count never exceeds the allocated middleware vector length; insertion precedes increment.
            .insert(self.caller_middlewares, Arc::new(middleware));
        // PANIC-SAFETY: The caller count never exceeds the allocated middleware vector length; insertion precedes increment.
        self.caller_middlewares += 1;
        self
    }

    /// The whole chain, ending at the chosen transport or the engine's default HTTP client.
    #[doc(hidden)]
    pub fn next<'a>(&'a self, default: &'a impl Transport) -> Next<'a> {
        Next {
            middlewares: &self.middlewares,
            transport: self.transport.as_deref().unwrap_or(default),
        }
    }

    /// Resolve the lazy HTTP transport once, shared by all clones.
    pub(super) fn sending_next(&self, endpoint: &'static str) -> Result<Next<'_>, crate::Error<P>> {
        let transport = match self.transport.as_deref() {
            Some(transport) => transport,
            None => match self
                .default_transport
                .get_or_init(|| crate::Http::new().map_err(|e| Arc::new(e.without_url())))
            {
                Ok(http) => http,
                Err(cause) => return Err(crate::Error::http(endpoint, Box::new(cause.clone()))),
            },
        };
        Ok(Next {
            middlewares: &self.middlewares,
            transport,
        })
    }

    /// `with_retry` of a client.
    pub fn with_retry(mut self, retry: Retry<P>) -> Core<P> {
        self.retry = Some(retry);
        self
    }

    /// `with_timeout` of a client: the whole call.
    pub fn with_timeout(mut self, timeout: Duration) -> Core<P> {
        self.timeout = Some(timeout);
        self
    }

    /// `with_idle_timeout` of a client: silence.
    pub fn with_idle_timeout(mut self, timeout: Duration) -> Core<P> {
        self.idle_timeout = Some(timeout);
        self
    }

    /// `with_base` of a client. An invalid text is kept as a Build error for each call,
    /// so configuring the client remains infallible.
    pub fn with_base(mut self, base: impl IntoBase) -> Core<P> {
        self.base = Some(base.into_base());
        self
    }
}

type RequestEdit = dyn Fn(&mut reqwest::Request) + Send + Sync;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IdleSource {
    Call,
    Description,
    Client,
    Default,
}

/// What one call changes for itself: retry, clocks, a replacement URL and request edits.
/// Also retains the URL of the answer seen by a pager, for relative URL resolution.
pub struct Options<P> {
    retry: Option<Retry<P>>,
    description_idle_timeout: Option<Duration>,
    timeout: Option<Duration>,
    idle_timeout: Option<Duration>,
    at_url: Option<String>,
    edits: Vec<Arc<RequestEdit>>,
    answered_from: Option<reqwest::Url>,
    conditional_etag: Option<String>,
}

impl<P> fmt::Debug for Options<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options").finish_non_exhaustive()
    }
}

impl<P> Clone for Options<P> {
    fn clone(&self) -> Options<P> {
        Options {
            retry: self.retry.clone(),
            description_idle_timeout: self.description_idle_timeout,
            timeout: self.timeout,
            idle_timeout: self.idle_timeout,
            at_url: self.at_url.clone(),
            edits: self.edits.clone(),
            answered_from: self.answered_from.clone(),
            conditional_etag: self.conditional_etag.clone(),
        }
    }
}

impl<P> Default for Options<P> {
    fn default() -> Options<P> {
        Options {
            retry: None,
            description_idle_timeout: None,
            timeout: None,
            idle_timeout: None,
            at_url: None,
            edits: Vec::new(),
            answered_from: None,
            conditional_etag: None,
        }
    }
}

impl<P> Options<P> {
    /// Retain a conditional header to write after description headers, before edits.
    #[doc(hidden)]
    pub fn if_none_match(&mut self, etag: &str) {
        self.conditional_etag = Some(etag.to_owned());
    }

    pub(super) fn conditional_etag(&self) -> Option<&str> {
        self.conditional_etag.as_deref()
    }

    /// The nearest `idle_timeout` expression of the description, evaluated for this call.
    /// The call override takes precedence; this precedes the client and 30 second default.
    pub fn description_idle_timeout(&mut self, timeout: Duration) {
        self.description_idle_timeout = Some(timeout);
    }

    /// `with_retry` of a call.
    pub fn with_retry(&mut self, retry: Retry<P>) {
        self.retry = Some(retry);
    }

    /// `with_timeout` of a call.
    pub fn with_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    /// `with_idle_timeout` of a call. The description's expression still runs once
    /// per send, before `idempotent` and the request; this length takes precedence.
    pub fn with_idle_timeout(&mut self, timeout: Duration) {
        self.idle_timeout = Some(timeout);
    }

    /// The URL of the answer that a pager saw. Set on its next call before preparation.
    pub fn answered_from(&mut self, url: reqwest::Url) {
        self.answered_from = Some(url);
    }

    /// `at_url` of a call: replaces the URL the description builds. A relative URL
    /// resolves against `answered_from`, or the built URL when none was recorded,
    /// before the origin check and before caller edits.
    pub fn at_url(&mut self, url: &str) {
        self.at_url = Some(url.to_owned());
    }

    /// `edit_request` of a call. Each one adds; they run in the order written.
    /// Header values marked sensitive are also omitted from page request comparison.
    /// A changed URL loses its secret marks and is compared as it is.
    pub fn edit_request(&mut self, edit: impl Fn(&mut reqwest::Request) + Send + Sync + 'static) {
        self.edits.push(Arc::new(edit));
    }
}

impl<P> Core<P> {
    /// The caller's replacement base, including a deferred parse failure.
    pub(super) fn base(&self) -> Option<&Result<Base, BaseError>> {
        self.base.as_ref()
    }
}

impl<P> Options<P> {
    /// The call's retry policy, or the client's when the call has none.
    pub(super) fn retry<'a>(&'a self, core: &'a Core<P>) -> Option<&'a Retry<P>> {
        self.retry.as_ref().or(core.retry.as_ref())
    }
    /// The total-call timeout; absent until the caller sets it.
    pub(super) fn timeout(&self, core: &Core<P>) -> Option<Duration> {
        self.timeout.or(core.timeout)
    }
    /// The effective silence limit: call, description, client, then 30 seconds.
    pub(super) fn idle_timeout(&self, core: &Core<P>) -> Duration {
        self.idle_timeout
            .or(self.description_idle_timeout)
            .or(core.idle_timeout)
            .unwrap_or(Duration::from_secs(30))
    }

    pub(super) fn idle_source(&self, core: &Core<P>) -> IdleSource {
        if self.idle_timeout.is_some() {
            IdleSource::Call
        } else if self.description_idle_timeout.is_some() {
            IdleSource::Description
        } else if core.idle_timeout.is_some() {
            IdleSource::Client
        } else {
            IdleSource::Default
        }
    }
    /// Resolve a replacement URL against its answer, or the URL built by the call.
    pub(super) fn replacement_url(&self, built: &reqwest::Url) -> Result<Option<reqwest::Url>, ()> {
        self.at_url
            .as_deref()
            .map(|text| {
                self.answered_from
                    .as_ref()
                    .unwrap_or(built)
                    .join(super::without_fragment(text))
                    .map_err(|_| ())
            })
            .transpose()
    }
    /// Run caller edits in insertion order; report whether any edit changed the URL.
    pub(super) fn edit(&self, request: &mut reqwest::Request) -> bool {
        let mut changed = false;
        for edit in &self.edits {
            let before = request.url().clone();
            edit(request);
            changed |= request.url() != &before;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c7_3_default_http_is_lazy_and_shared_by_all_clones() {
        let core = Core::<()>::default();
        let clone = core.clone();
        assert!(core.default_transport.get().is_none());
        let custom = core
            .clone()
            .with_transport(crate::fake(|_| Ok(crate::reply(200, ""))));
        custom.sending_next("contract.http").unwrap();
        assert!(core.default_transport.get().is_none());
        let first = core.sending_next("contract.http").unwrap();
        let second = clone.sending_next("contract.http").unwrap();
        assert!(std::ptr::eq(first.transport, second.transport));
        assert!(core.default_transport.get().is_some());
    }

    #[test]
    fn c7_3_default_http_construction_failure_is_a_build_error_with_zero_tries() {
        let core = Core::<()>::default();
        let error = reqwest::Client::builder()
            .user_agent("\n")
            .build()
            .unwrap_err();
        core.default_transport
            .set(Err(Arc::new(error.without_url())))
            .unwrap();
        let error = core.sending_next("contract.http").unwrap_err();
        assert_eq!(error.kind(), crate::error::Kind::Build);
        assert_eq!(error.tries(), 0);
        assert_eq!(
            error.to_string(),
            "build error: the HTTP client could not be built, in contract.http"
        );
        assert!(std::error::Error::source(&error).is_some());
        assert!(core.clone().sending_next("contract.http").is_err());
    }

    #[test]
    fn description_idle_timeout_is_retained_by_options_clones() {
        let mut options = Options::<()>::default();
        assert_eq!(options.description_idle_timeout, None);
        options.description_idle_timeout(Duration::from_secs(120));
        assert_eq!(
            options.clone().description_idle_timeout,
            Some(Duration::from_secs(120))
        );
    }
    #[test]
    /// Also proves C4.6.
    fn c7_6_client_settings_and_call_precedence_survive_clone() {
        let core = Core::<()>::default()
            .with_timeout(Duration::from_secs(60))
            .with_idle_timeout(Duration::from_secs(40));
        let mut options = Options::default();
        assert_eq!(
            options.timeout(&core.clone()),
            Some(Duration::from_secs(60))
        );
        assert_eq!(options.idle_timeout(&core), Duration::from_secs(40));
        options.description_idle_timeout(Duration::from_secs(20));
        assert_eq!(options.idle_timeout(&core), Duration::from_secs(20));
        options.with_timeout(Duration::ZERO);
        options.with_idle_timeout(Duration::from_secs(10));
        let copy = options.clone();
        assert_eq!(copy.timeout(&core), Some(Duration::ZERO));
        assert_eq!(copy.idle_timeout(&core), Duration::from_secs(10));
        assert_eq!(Options::<()>::default().timeout(&Core::default()), None);
        assert_eq!(
            Options::<()>::default().idle_timeout(&Core::default()),
            Duration::from_secs(30)
        );
        let core = core.with_retry(Retry::standard());
        let mut options = Options::default();
        assert!(std::ptr::eq(
            options.retry(&core).unwrap(),
            core.retry.as_ref().unwrap()
        ));
        options.with_retry(Retry::standard());
        assert!(std::ptr::eq(
            options.retry(&core).unwrap(),
            options.retry.as_ref().unwrap()
        ));
        assert!(options.clone().retry(&core.clone()).is_some());
    }
}
