use std::fmt;
use std::future::Future;
use std::hash::{BuildHasher, Hasher};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::{Duration, SystemTime};
use tokio::time::Instant;

use super::client::IdleSource;
use super::{Core, Idempotent, Options};
use crate::answer::{Answer, BodyClock, BodyTimeout, Headers, KnownKind, KnownOutcome};
use crate::error::{Clock, Error, Kept, Kind, MismatchFailure};
use crate::retry::{again, Decision, Outcome, Retry};
use crate::transport::TryInfo;

/// The clocks of one call, shared by its tries, readers and handed-over stream.
#[derive(Clone)]
pub struct Run {
    endpoint: &'static str,
    total: Option<(Instant, Duration)>,
    idle: Duration,
    idle_source: IdleSource,
    known: Arc<Mutex<Option<Evidence>>>,
}

struct Evidence {
    kind: KnownKind,
    status: http::StatusCode,
    headers: Headers,
    body: bytes::Bytes,
    kept: Kept,
}

impl fmt::Debug for Run {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Run")
            .field("endpoint", &self.endpoint)
            .finish_non_exhaustive()
    }
}

impl Run {
    /// Starts the whole-call clock before the request is built.
    pub fn start<P>(core: &Core<P>, options: &Options<P>, endpoint: &'static str) -> Run {
        Run {
            endpoint,
            total: options
                .timeout(core)
                .and_then(|length| Instant::now().checked_add(length).map(|end| (end, length))),
            idle: options.idle_timeout(core),
            idle_source: options.idle_source(core),
            known: Arc::new(Mutex::new(None)),
        }
    }

    pub(super) fn finish_options<P>(&mut self, core: &Core<P>, options: &Options<P>) {
        // The description expression ran after start(), and must not restart timeout.
        self.idle = options.idle_timeout(core);
        self.idle_source = options.idle_source(core);
    }

    pub(crate) fn endpoint(&self) -> &'static str {
        self.endpoint
    }

    pub(crate) fn expired(&self) -> bool {
        self.total.is_some_and(|(end, _)| Instant::now() >= end)
    }

    pub(crate) fn total_timeout(&self) -> BodyTimeout {
        BodyTimeout::Total(self.total.map(|(_, length)| length).unwrap_or_default())
    }

    pub(crate) fn idle(&self) -> Duration {
        self.idle
    }

    pub(crate) fn idle_timeout(&self) -> BodyTimeout {
        match self.idle_source {
            IdleSource::Description => BodyTimeout::IdleFromDescription(self.idle),
            _ => BodyTimeout::Idle(self.idle),
        }
    }

    // A57: when clocks end together, the clock covering more of the call wins.
    // Ready answers still win in bounded(); only an inner clock failure is promoted.
    pub(crate) fn outer_clock(
        &self,
        failure: crate::transport::TransportError,
    ) -> crate::transport::TransportError {
        if self.expired()
            && (failure.clock_timeout().is_some() || failure.connect_timeout().is_some())
        {
            crate::transport::TransportError::clock(self.total_timeout())
        } else {
            failure
        }
    }

    fn clear(&self) {
        *self
            .known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }

    fn timeout_error<P>(&self, n: u32) -> Error<P> {
        let known = self
            .known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(known) = known {
            let error = match known.kind {
                KnownKind::Refused => {
                    Error::refused(self.endpoint, n, known.status, known.headers, None)
                }
                KnownKind::NoArm => Error::mismatch(
                    self.endpoint,
                    n,
                    known.status,
                    known.headers,
                    MismatchFailure::NoArm,
                ),
            };
            return error.with_body(&known.body, known.kept);
        }
        Error::clock(
            self.endpoint,
            n,
            Clock::Total,
            self.total.map(|(_, length)| length).unwrap_or_default(),
        )
    }

    async fn bounded<F: Future>(&self, work: F) -> Result<F::Output, ()> {
        let Some((end, _)) = self.total else {
            return Ok(work.await);
        };
        let work = std::pin::pin!(work);
        let timer = std::pin::pin!(tokio::time::sleep_until(end));
        let (mut work, mut timer) = (work, timer);
        std::future::poll_fn(|cx| {
            if let Poll::Ready(value) = work.as_mut().poll(cx) {
                return Poll::Ready(Ok(value));
            }
            timer.as_mut().poll(cx).map(|()| Err(()))
        })
        .await
    }
}

impl BodyClock for Run {
    fn wait(&self) -> Pin<Box<dyn Future<Output = BodyTimeout> + Send>> {
        let idle = self.idle;
        let idle_timeout = self.idle_timeout();
        let idle_end = Instant::now().checked_add(idle);
        let total = self.total;
        Box::pin(async move {
            let (end, timeout) = match (total, idle_end) {
                (Some((end, length)), Some(idle_end)) if end <= idle_end => {
                    (Some(end), BodyTimeout::Total(length))
                }
                (_, Some(end)) => (Some(end), idle_timeout),
                (Some((end, length)), None) => (Some(end), BodyTimeout::Total(length)),
                (None, None) => (None, idle_timeout),
            };
            if let Some(end) = end {
                tokio::time::sleep_until(end).await;
            } else {
                std::future::pending::<()>().await;
            }
            timeout
        })
    }

    fn record(&self, evidence: KnownOutcome<'_>) {
        *self
            .known
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Evidence {
            kind: evidence.kind,
            status: evidence.status,
            headers: evidence.headers.clone(),
            body: bytes::Bytes::copy_from_slice(
                evidence
                    .body
                    // PANIC-SAFETY: 64 KiB is representable on supported 32/64-bit targets.
                    .get(..evidence.body.len().min(64 * crate::KIB))
                    .unwrap_or_default(),
            ),
            kept: evidence.kept,
        });
    }
}

fn error_outcome<P>(error: &Error<P>) -> Outcome<'_, P> {
    Outcome {
        kind: error.kind(),
        status: error.status(),
        class: error.class(),
        headers: error.headers(),
        problem: error.problem(),
    }
}

fn log_retry<P>(endpoint: &'static str, n: u32, outcome: Outcome<'_, P>, wait: Duration) {
    tracing::debug!(target: "concord", endpoint, r#try = n, status = outcome.status.map(|s| s.as_u16()), class = ?outcome.class, wait = ?wait, "retry");
}

// RandomState supplies independently seeded hash keys. Hashing the attempt number
// gives a fresh uniform fraction without process-wide engine state or a new RNG dependency.
fn random(state: &impl BuildHasher, n: u32) -> f64 {
    let mut hash = state.build_hasher();
    hash.write_u32(n);
    // PANIC-SAFETY: Shifts are below 64; the 53-bit numerator/denominator fit f64 exactly and the denominator is nonzero.
    (hash.finish() >> 11) as f64 / ((1u64 << 53) - 1) as f64
}

/// The middleware chain, transport, reader, and retries around the whole exchange.
/// Each try sends a fresh copy of the one finished request.
pub async fn exchange<'a, P, T, F, Fut>(
    run: Run,
    core: &'a Core<P>,
    options: &'a Options<P>,
    idempotent: Idempotent,
    request: reqwest::Request,
    one_try: F,
) -> Result<Answer<T>, Error<P>>
where
    P: Send + Sync + 'a,
    T: Send + 'a,
    F: Fn(reqwest::Response, TryInfo) -> Fut + Send + Sync + 'a,
    Fut: Future<Output = Result<Answer<T>, Error<P>>> + Send + 'a,
{
    exchange_with(
        run,
        core,
        options,
        idempotent,
        request,
        |response: reqwest::Response, info| {
            let one_try = &one_try;
            async move {
                let url = response.url().clone();
                match one_try(response, info).await {
                    Ok(answer) => Attempt::Done(answer.with_response_url(url)),
                    Err(error) => Attempt::Failed(error),
                }
            }
        },
    )
    .await
}

/// Raw exchange: retry considers refusal heads but never reads a response body.
pub async fn exchange_raw<'a, P>(
    run: Run,
    core: &'a Core<P>,
    options: &'a Options<P>,
    idempotent: Idempotent,
    request: reqwest::Request,
) -> Result<reqwest::Response, Error<P>>
where
    P: Send + Sync + 'a,
{
    exchange_with(
        run,
        core,
        options,
        idempotent,
        request,
        |response: reqwest::Response, _| async move {
            if response.status().as_u16() < 400 {
                Attempt::Done(response)
            } else {
                let status = response.status();
                let headers = Headers::from(response.headers().clone());
                Attempt::Refused(response, status, headers)
            }
        },
    )
    .await
}

// Reading is the only difference between typed and raw exchanges. A raw refusal
// retains the original response, including its unread body, while retries wait.
enum Attempt<T, P> {
    Done(T),
    Refused(T, http::StatusCode, Headers),
    Failed(Error<P>),
}
impl<T, P> Attempt<T, P> {
    fn outcome(&self) -> Option<Outcome<'_, P>> {
        match self {
            Self::Done(_) => None,
            Self::Refused(_, status, headers) => Some(Outcome {
                kind: Kind::Refused,
                status: Some(*status),
                class: None,
                headers: Some(headers),
                problem: None,
            }),
            Self::Failed(error) => Some(error_outcome(error)),
        }
    }
    fn finish(self, decision: Decision) -> Result<T, Error<P>> {
        match self {
            Self::Done(value) | Self::Refused(value, _, _) => Ok(value),
            Self::Failed(error) => {
                Err(error.with_retry_after(decision.header_wait, decision.rule_wait))
            }
        }
    }
}

async fn exchange_with<P, T, F, Fut>(
    run: Run,
    core: &Core<P>,
    options: &Options<P>,
    idempotent: Idempotent,
    request: reqwest::Request,
    read: F,
) -> Result<T, Error<P>>
where
    P: Send + Sync,
    T: Send,
    F: Fn(reqwest::Response, TryInfo) -> Fut + Send + Sync,
    Fut: Future<Output = Attempt<T, P>> + Send,
{
    exchange_with_random(
        run,
        core,
        options,
        idempotent,
        request,
        read,
        std::collections::hash_map::RandomState::new(),
    )
    .await
}

async fn exchange_with_random<P, T, F, Fut, R>(
    run: Run,
    core: &Core<P>,
    options: &Options<P>,
    idempotent: Idempotent,
    request: reqwest::Request,
    read: F,
    random_state: R,
) -> Result<T, Error<P>>
where
    P: Send + Sync,
    T: Send,
    F: Fn(reqwest::Response, TryInfo) -> Fut + Send + Sync,
    Fut: Future<Output = Attempt<T, P>> + Send,
    R: BuildHasher + Send + Sync,
{
    let method = request.method().clone();
    let repeat = request.try_clone();
    let mut request = Some(request);
    let none = Retry::none();
    let policy = options.retry(core).unwrap_or(&none);
    let mut n = 0u32;
    loop {
        let this = request
            .take()
            .or_else(|| repeat.as_ref().and_then(reqwest::Request::try_clone));
        let Some(this) = this else {
            return Err(run.timeout_error(n));
        };
        let url = this.url().clone();
        let next = core.sending_next(run.endpoint)?;
        n = n.saturating_add(1);
        run.clear();
        let info = TryInfo::for_run(run.clone(), n);
        let attempt = async {
            match next.send(this, info.clone()).await {
                Ok(response) => read(crate::transport::__response_at(response, url), info).await,
                Err(failure) => Attempt::Failed(Error::transport(run.endpoint, n, failure)),
            }
        };
        let result = match run.bounded(attempt).await {
            Ok(result) => result,
            Err(()) => Attempt::Failed(run.timeout_error(n)),
        };
        let Some(outcome) = result.outcome() else {
            return result.finish(Decision {
                wait: None,
                header_wait: None,
                rule_wait: None,
            });
        };
        // Rules may consume time; sample the wait deadline only after they run.
        let decision = again(
            policy,
            outcome,
            n,
            &method,
            idempotent,
            repeat.is_some(),
            None,
            SystemTime::now(),
            random(&random_state, n),
        );
        let until = decision
            .wait
            .and_then(|wait| Instant::now().checked_add(wait));
        let Some(until) = until.filter(|until| run.total.is_none_or(|(end, _)| *until < end))
        else {
            return result.finish(decision);
        };
        if let Some(wait) = decision.wait {
            if let Some(outcome) = result.outcome() {
                log_retry(run.endpoint, n, outcome, wait);
            }
        }
        // A slow log sink must neither move the deadline nor discard the last
        // outcome. Keep it until the wait completes and another send can start.
        if run.expired()
            || run.bounded(tokio::time::sleep_until(until)).await.is_err()
            || run.expired()
        {
            return result.finish(decision);
        }
        drop(result);
        run.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{Transport, TransportError};
    use crate::{Middleware, Next};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Clone, Default)]
    struct Draws(Arc<Mutex<Vec<u32>>>);
    struct DrawHasher(Draws, u32);
    impl BuildHasher for Draws {
        type Hasher = DrawHasher;
        fn build_hasher(&self) -> DrawHasher {
            DrawHasher(self.clone(), 0)
        }
    }
    impl Hasher for DrawHasher {
        fn write(&mut self, _: &[u8]) {
            panic!("the source hashes attempt numbers")
        }
        fn write_u32(&mut self, n: u32) {
            self.0 .0.lock().unwrap().push(n);
            self.1 = n;
        }
        fn finish(&self) -> u64 {
            u64::from(self.1) << 62
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c4_3_a56_each_retry_wait_uses_a_fresh_draw_from_a_fixed_source() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let times = sent.clone();
        let core = Core::<()>::default()
            .with_retry(Retry::standard())
            .with_transport(crate::fake(move |_| {
                let mut times = times.lock().unwrap();
                times.push(Instant::now());
                Ok(crate::reply(if times.len() < 3 { 503 } else { 200 }, ""))
            }));
        let options = Options::default();
        let run = Run::start(&core, &options, "jitter.get");
        let request = draft()
            .finish_for_send(&core, &options, "jitter.get")
            .unwrap();
        let draws = Draws::default();
        let response = exchange_with_random(
            run,
            &core,
            &options,
            Idempotent::ByMethod,
            request,
            |response, _| async move {
                if response.status() == 503 {
                    Attempt::Refused(
                        response,
                        http::StatusCode::SERVICE_UNAVAILABLE,
                        Headers::default(),
                    )
                } else {
                    Attempt::Done(response)
                }
            },
            draws.clone(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(*draws.0.lock().unwrap(), [1, 2]);
        let times = sent.lock().unwrap();
        assert_eq!(times[1] - times[0], Duration::from_millis(250));
        assert_eq!(times[2] - times[1], Duration::from_secs(1));
    }

    #[tokio::test(start_paused = true)]
    async fn c7_6_a57_total_wins_a_body_idle_tie() {
        for description in [false, true] {
            let length = Duration::from_secs(5);
            let core = Core::<()>::default()
                .with_timeout(length)
                .with_idle_timeout(length);
            let mut options = Options::default();
            if description {
                options.description_idle_timeout(length);
            }
            let run = Run::start(&core, &options, "tie.body");
            let start = Instant::now();
            assert!(matches!(run.wait().await, BodyTimeout::Total(d) if d == length));
            assert_eq!(start.elapsed(), length);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c7_6_a57_total_wins_a_head_idle_tie() {
        struct Silent;
        impl Transport for Silent {
            async fn send(
                &self,
                _: reqwest::Request,
                _: TryInfo,
            ) -> Result<reqwest::Response, TransportError> {
                std::future::pending().await
            }
        }
        let length = Duration::from_secs(5);
        let core = Core::<()>::default()
            .with_timeout(length)
            .with_idle_timeout(length)
            .with_transport(Silent);
        let options = Options::default();
        let run = Run::start(&core, &options, "tie.head");
        let request = draft()
            .finish_for_send(&core, &options, "tie.head")
            .unwrap();
        let start = Instant::now();
        let failure = core
            .sending_next(run.endpoint())
            .unwrap()
            .send(request, TryInfo::for_run(run, 1))
            .await
            .unwrap_err();
        let error: Error = Error::transport("tie.head", 1, failure);
        assert_eq!(start.elapsed(), length);
        assert_eq!(
            error.to_string(),
            Error::<()>::clock("tie.head", 1, Clock::Total, length).to_string()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn c7_1_a57_total_wins_a_connect_timeout_tie_without_a_socket() {
        struct PendingResolver;
        impl reqwest::dns::Resolve for PendingResolver {
            fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
                Box::pin(std::future::pending())
            }
        }
        let length = Duration::from_secs(30);
        let transport =
            crate::Http::with(|b| b.no_proxy().dns_resolver(Arc::new(PendingResolver))).unwrap();
        let core = Core::<()>::default()
            .with_timeout(length)
            .with_idle_timeout(Duration::from_secs(60))
            .with_transport(transport);
        let options = Options::default();
        let run = Run::start(&core, &options, "tie.connect");
        let request = reqwest::Request::new(
            http::Method::GET,
            "http://never-resolved.test/".parse().unwrap(),
        );
        let start = Instant::now();
        let failure = core
            .sending_next(run.endpoint())
            .unwrap()
            .send(request, TryInfo::for_run(run, 1))
            .await
            .unwrap_err();
        let error: Error = Error::transport("tie.connect", 1, failure);
        assert_eq!(start.elapsed(), length);
        assert_eq!(
            error.to_string(),
            Error::<()>::clock("tie.connect", 1, Clock::Total, length).to_string()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn c4_3_a57_a_retry_wait_tied_with_total_cannot_start_another_try() {
        let length = Duration::from_secs(5);
        let core = Core::<()>::default()
            .with_timeout(length)
            .with_retry(Retry::standard().when(move |_| Some(length)))
            .with_transport(crate::fake(|_| Ok(crate::reply(503, "last outcome"))));
        let options = Options::default();
        let run = Run::start(&core, &options, "tie.wait");
        let request = draft()
            .finish_for_send(&core, &options, "tie.wait")
            .unwrap();
        let start = Instant::now();
        let error =
            exchange::<(), (), _, _>(
                run,
                &core,
                &options,
                Idempotent::ByMethod,
                request,
                |response, info| async move {
                    Err(crate::__private::refused_plain(response, &info).await)
                },
            )
            .await
            .unwrap_err();
        // A43 retains the last outcome when the total clock disallows a wait.
        assert_eq!(error.kind(), Kind::Refused);
        assert_eq!(error.tries(), 1);
        assert_eq!(error.retry_after(), Some(length));
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    fn draft() -> super::super::Draft {
        let mut draft = super::super::Draft::new(http::Method::GET);
        draft.base(&[super::super::Piece::Lit("https://private.invalid/PRIVATE")]);
        draft
    }

    #[tokio::test(start_paused = true)]
    async fn c10_10_preparation_selects_description_idle_without_restarting_total() {
        let core = Core::<()>::default()
            .with_timeout(Duration::from_secs(2))
            .with_transport(crate::fake(|_| Ok(crate::reply(200, "ready"))));
        let mut options = Options::default();
        let start = Instant::now();
        let run = Run::start(&core, &options, "contract.prepare");
        tokio::time::advance(Duration::from_secs(1)).await;
        options.description_idle_timeout(Duration::from_secs(100));
        let prepared = draft()
            .prepare(
                run,
                core,
                options,
                Idempotent::Yes,
                |response, info| async move {
                    let _: bytes::Bytes =
                        crate::__private::read_body(&WaitingReader, response, &info).await?;
                    Ok(Answer::new(http::StatusCode::OK, Headers::default(), ()))
                },
            )
            .unwrap();
        assert_eq!(prepared.run.idle(), Duration::from_secs(100));
        let error = prepared.send().await.unwrap_err();
        assert_eq!(start.elapsed(), Duration::from_secs(2));
        assert!(error.is_timeout());
        assert_eq!(error.tries(), 1);
    }

    struct WaitingReader;
    impl crate::Reader<bytes::Bytes> for WaitingReader {
        async fn read(
            &self,
            _: crate::read::Incoming,
        ) -> Result<bytes::Bytes, crate::read::ReadError> {
            std::future::pending().await
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c7_6_only_total_bounds_a_readers_own_wait() {
        let core = Core::<()>::default()
            .with_timeout(Duration::from_secs(50))
            .with_idle_timeout(Duration::from_secs(1))
            .with_transport(crate::fake(|_| Ok(crate::reply(200, "ready"))));
        let options = Options::default();
        let run = Run::start(&core, &options, "contract.reader");
        let start = Instant::now();
        let result = exchange(
            run,
            &core,
            &options,
            Idempotent::ByMethod,
            draft()
                .finish_for_send(&core, &options, "contract.reader")
                .unwrap(),
            |response, info| async move {
                crate::__private::read_body::<bytes::Bytes, (), _>(&WaitingReader, response, &info)
                    .await?;
                Ok(Answer::new(http::StatusCode::OK, Headers::default(), ()))
            },
        )
        .await;
        assert_eq!(start.elapsed(), Duration::from_secs(50));
        assert!(result.unwrap_err().is_timeout());
    }

    #[tokio::test(start_paused = true)]
    async fn c7_6_no_arm_survives_total_cancellation_with_progressive_evidence() {
        let core = Core::<()>::default()
            .with_timeout(Duration::from_secs(2))
            .with_idle_timeout(Duration::from_secs(100))
            .with_transport(crate::fake(|_| {
                let stream = futures_util::stream::once(async {
                    Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"prefix"))
                });
                let stream =
                    futures_util::StreamExt::chain(stream, futures_util::stream::pending());
                Ok(crate::reply_with(
                    201,
                    &[("x-evidence", "kept")],
                    reqwest::Body::wrap_stream(stream),
                ))
            }));
        let options = Options::default();
        let run = Run::start(&core, &options, "contract.no_arm");
        let result: Result<Answer<()>, Error> = exchange(
            run,
            &core,
            &options,
            Idempotent::ByMethod,
            draft()
                .finish_for_send(&core, &options, "contract.no_arm")
                .unwrap(),
            |response, info| async move { Err(crate::__private::no_arm(response, &info).await) },
        )
        .await;
        let error = result.unwrap_err();
        assert_eq!(error.kind(), Kind::Mismatch);
        assert!(error.is_status(201));
        assert_eq!(error.headers().unwrap().get("x-evidence"), Some("kept"));
        assert_eq!(error.body(), Some(b"prefix".as_slice()));
        assert_eq!(error.kept(), Some(Kept::Interrupted));
    }

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Stage {
        Before,
        Transport,
        Upload,
        After,
        Reader,
        Body,
        Retry,
    }

    struct DropNotice(Arc<AtomicUsize>);
    impl Drop for DropNotice {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    struct Stages {
        stage: Stage,
        dropped: Arc<AtomicUsize>,
        sends: Arc<AtomicUsize>,
    }
    impl Transport for Stages {
        async fn send(
            &self,
            mut request: reqwest::Request,
            _: TryInfo,
        ) -> Result<reqwest::Response, TransportError> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            if self.stage == Stage::Transport {
                let _notice = DropNotice(self.dropped.clone());
                std::future::pending::<()>().await;
            }
            if self.stage == Stage::Upload {
                let body = request.body_mut().take().unwrap();
                reqwest::Response::from(http::Response::new(body))
                    .bytes()
                    .await
                    .map_err(TransportError::from)?;
            }
            if self.stage == Stage::Retry {
                return Ok(crate::reply_with(503, &[("retry-after", "10")], ""));
            }
            if self.stage == Stage::Body {
                let notice = DropNotice(self.dropped.clone());
                return Ok(crate::reply(
                    200,
                    reqwest::Body::wrap_stream(futures_util::stream::once(async move {
                        let _notice = notice;
                        std::future::pending::<Result<bytes::Bytes, std::io::Error>>().await
                    })),
                ));
            }
            Ok(crate::reply(200, "ready"))
        }
    }
    struct Waits {
        stage: Stage,
        dropped: Arc<AtomicUsize>,
    }
    impl Middleware for Waits {
        async fn handle(
            &self,
            request: reqwest::Request,
            info: TryInfo,
            next: Next<'_>,
        ) -> Result<reqwest::Response, TransportError> {
            if self.stage == Stage::Before {
                let _notice = DropNotice(self.dropped.clone());
                std::future::pending::<()>().await;
            }
            let answer = next.send(request, info).await?;
            if self.stage == Stage::After {
                let _notice = DropNotice(self.dropped.clone());
                std::future::pending::<()>().await;
            }
            Ok(answer)
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c7_7_drop_stops_at_every_await_boundary() {
        use futures_util::FutureExt;
        for stage in [
            Stage::Before,
            Stage::Transport,
            Stage::Upload,
            Stage::After,
            Stage::Reader,
            Stage::Body,
            Stage::Retry,
        ] {
            let dropped = Arc::new(AtomicUsize::new(0));
            let sends = Arc::new(AtomicUsize::new(0));
            let core = Core::<()>::default()
                .with_retry(Retry::standard())
                .with_transport(Stages {
                    stage,
                    dropped: dropped.clone(),
                    sends: sends.clone(),
                })
                .with_middleware(Waits {
                    stage,
                    dropped: dropped.clone(),
                });
            let options = Options::default();
            let run = Run::start(&core, &options, "contract.drop");
            let notice = dropped.clone();
            let mut draft = draft();
            if stage == Stage::Upload {
                let notice = DropNotice(dropped.clone());
                draft.body(crate::Body::stream(futures_util::stream::once(
                    async move {
                        let _notice = notice;
                        std::future::pending::<Result<bytes::Bytes, std::io::Error>>().await
                    },
                )));
            }
            let sending = draft
                .prepare(
                    run,
                    core,
                    options,
                    Idempotent::Yes,
                    move |response, info| {
                        let notice = notice.clone();
                        async move {
                            if stage == Stage::Reader {
                                let _notice = DropNotice(notice);
                                std::future::pending::<()>().await;
                            }
                            if !response.status().is_success() {
                                return Err(crate::__private::refused_plain(response, &info).await);
                            }
                            let incoming = crate::read::Incoming::new(response).with_info(&info);
                            incoming.bytes(crate::KIB).await.map_err(|e| {
                                e.into_error(http::StatusCode::OK, Headers::default(), &info)
                            })?;
                            Ok(Answer::new(http::StatusCode::OK, Headers::default(), ()))
                        }
                    },
                )
                .unwrap()
                .send();
            assert!(sending.now_or_never().is_none(), "{stage:?}");
            assert_eq!(
                dropped.load(Ordering::SeqCst),
                usize::from(stage != Stage::Retry),
                "{stage:?}"
            );
            let count = sends.load(Ordering::SeqCst);
            tokio::time::advance(Duration::from_secs(100)).await;
            assert_eq!(sends.load(Ordering::SeqCst), count, "{stage:?}");
            assert_eq!(count, usize::from(stage != Stage::Before), "{stage:?}");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c10_6_retry_is_the_only_event_and_has_no_request_or_response_values() {
        use tracing::{
            field::{Field, Visit},
            subscriber::Subscriber,
            Event, Metadata,
        };
        #[derive(Clone)]
        struct Logs(Arc<Mutex<Vec<String>>>);
        struct Fields(String);
        impl Visit for Fields {
            fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
                use fmt::Write;
                write!(&mut self.0, " {}={value:?}", field.name()).unwrap();
            }
        }
        impl Subscriber for Logs {
            fn enabled(&self, _: &Metadata<'_>) -> bool {
                true
            }
            fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
                tracing::span::Id::from_u64(1)
            }
            fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
            fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
            fn event(&self, event: &Event<'_>) {
                let mut fields = Fields(format!(
                    "{} {}",
                    event.metadata().target(),
                    event.metadata().level()
                ));
                event.record(&mut fields);
                self.0.lock().unwrap().push(fields.0);
            }
            fn enter(&self, _: &tracing::span::Id) {}
            fn exit(&self, _: &tracing::span::Id) {}
        }
        let logs = Logs(Arc::new(Mutex::new(Vec::new())));
        // tracing-core's one-dispatch optimization can cache another thread's
        // NoSubscriber interest for our shared retry callsite. Retain a second,
        // inert dispatch so registration always merges interests across live
        // subscribers, independently of other libtest threads' defaults.
        let _inert = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
        let dispatch = tracing::Dispatch::new(logs.clone());
        let _guard = tracing::dispatcher::set_default(&dispatch);
        std::thread::spawn(|| {
            log_retry::<()>(
                "unrelated.thread",
                1,
                Outcome {
                    kind: Kind::Refused,
                    status: Some(http::StatusCode::SERVICE_UNAVAILABLE),
                    class: None,
                    headers: None,
                    problem: None,
                },
                Duration::ZERO,
            );
        })
        .join()
        .unwrap();
        let sends = Arc::new(AtomicUsize::new(0));
        let count = sends.clone();
        let core = Core::<()>::default()
            .with_retry(Retry::standard())
            .with_transport(crate::fake(move |_| {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    Ok(crate::reply_with(
                        503,
                        &[("x-private", "PRIVATE")],
                        "PRIVATE",
                    ))
                } else {
                    Ok(crate::reply(200, ""))
                }
            }));
        let options = Options::default();
        let run = Run::start(&core, &options, "contract.log");
        exchange(
            run,
            &core,
            &options,
            Idempotent::ByMethod,
            draft()
                .finish_for_send(&core, &options, "contract.log")
                .unwrap(),
            |response, info| async move {
                if !response.status().is_success() {
                    return Err(crate::__private::refused_plain(response, &info).await);
                }
                Ok(Answer::new(
                    response.status(),
                    response.headers().clone().into(),
                    (),
                ))
            },
        )
        .await
        .unwrap();
        let events = logs.0.lock().unwrap();
        let events: Vec<_> = events
            .iter()
            .filter(|line| line.starts_with("concord "))
            .collect();
        assert_eq!(events.len(), 1);
        let line = events.first().unwrap();
        for field in [
            "DEBUG",
            "retry",
            "endpoint=\"contract.log\"",
            "try=1",
            "status=503",
            "class=None",
            "wait=",
        ] {
            assert!(line.contains(field), "{line}");
        }
        assert!(!line.contains("PRIVATE"));
        assert_eq!(sends.load(Ordering::SeqCst), 2);
    }
    struct SlowLog(Duration);
    impl tracing::Subscriber for SlowLog {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            if event.metadata().target() == "concord" {
                let runtime = tokio::runtime::Handle::current();
                let delay = self.0;
                // Advance the paused runtime from outside its current thread:
                // this models synchronous time spent in a logging sink.
                std::thread::spawn(move || runtime.block_on(tokio::time::advance(delay)))
                    .join()
                    .unwrap();
            }
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[tokio::test(start_paused = true)]
    async fn c4_3_a43_a_retry_wait_has_one_deadline_even_with_a_slow_log_sink() {
        for raw in [false, true] {
            for delay in [
                Duration::ZERO,
                Duration::from_millis(500),
                Duration::from_millis(1600),
            ] {
                let _inert = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
                let dispatch = tracing::Dispatch::new(SlowLog(delay));
                let _guard = tracing::dispatcher::set_default(&dispatch);
                let sends = Arc::new(AtomicUsize::new(0));
                let count = sends.clone();
                let core = Core::<()>::default()
                    .with_timeout(Duration::from_millis(1500))
                    .with_retry(Retry::standard().when(|_| Some(Duration::from_secs(1))))
                    .with_transport(crate::fake(move |_| {
                        count.fetch_add(1, Ordering::SeqCst);
                        Ok(crate::reply_with(503, &[("x-evidence", "kept")], "busy"))
                    }));
                let options = Options::default();
                let run = Run::start(&core, &options, "contract.wait");
                let request = draft()
                    .finish_for_send(&core, &options, "contract.wait")
                    .unwrap();
                let start = Instant::now();
                let tries = if raw {
                    let response =
                        exchange_raw(run, &core, &options, Idempotent::ByMethod, request)
                            .await
                            .unwrap();
                    assert_eq!(response.status(), 503);
                    assert_eq!(response.headers()["x-evidence"], "kept");
                    assert_eq!(response.bytes().await.unwrap(), "busy");
                    sends.load(Ordering::SeqCst)
                } else {
                    let error = exchange::<(), (), _, _>(
                        run,
                        &core,
                        &options,
                        Idempotent::ByMethod,
                        request,
                        |response, info| async move {
                            Err(crate::__private::refused_plain(response, &info).await)
                        },
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(error.kind(), Kind::Refused);
                    assert_eq!(error.status(), Some(http::StatusCode::SERVICE_UNAVAILABLE));
                    assert_eq!(error.headers().unwrap().get("x-evidence").unwrap(), "kept");
                    assert_eq!(error.body(), Some(b"busy".as_slice()));
                    error.tries() as usize
                };
                let expected = if delay < Duration::from_millis(1500) {
                    2
                } else {
                    1
                };
                assert_eq!(tries, expected, "raw={raw}, delay={delay:?}");
                assert_eq!(sends.load(Ordering::SeqCst), expected);
                if expected == 2 {
                    // A one-second rule dominates first-try jitter. The reader's
                    // slow-log probe is deterministic on the paused clock.
                    assert_eq!(start.elapsed(), Duration::from_secs(1));
                } else {
                    assert_eq!(start.elapsed(), delay);
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c4_3_a_started_wait_cut_by_timeout_keeps_the_last_outcome_for_both_readers() {
        for raw in [false, true] {
            let core = Core::<()>::default()
                .with_timeout(Duration::from_secs(1))
                .with_retry(Retry::standard().when(|_| Some(Duration::from_millis(600))))
                .with_transport(crate::fake(|_| Ok(crate::reply(503, "busy"))));
            let options = Options::default();
            let run = Run::start(&core, &options, "contract.wait");
            let request = draft()
                .finish_for_send(&core, &options, "contract.wait")
                .unwrap();
            let work = async {
                if raw {
                    let response =
                        exchange_raw(run, &core, &options, Idempotent::ByMethod, request)
                            .await
                            .unwrap();
                    assert_eq!(response.status(), 503);
                    assert_eq!(response.bytes().await.unwrap(), "busy");
                } else {
                    let error = exchange::<(), (), _, _>(
                        run,
                        &core,
                        &options,
                        Idempotent::ByMethod,
                        request,
                        |response, info| async move {
                            Err(crate::__private::refused_plain(response, &info).await)
                        },
                    )
                    .await
                    .unwrap_err();
                    assert_eq!(error.kind(), Kind::Refused);
                    assert_eq!(error.tries(), 1);
                    assert_eq!(error.body(), Some(b"busy".as_slice()));
                }
            };
            let mut work = Box::pin(work);
            assert!(
                std::future::poll_fn(|cx| Poll::Ready(work.as_mut().poll(cx).is_pending())).await
            );
            tokio::time::advance(Duration::from_secs(1)).await;
            work.await;
        }
    }
}
