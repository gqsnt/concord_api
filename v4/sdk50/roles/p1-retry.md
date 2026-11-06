# Capability capsule: `concord-retry` — one bounded repetition owner, built on Concord's public API

You implement a new focused crate in the Concord Rust workspace (your working directory, a git
worktree on branch `sdk50/p1-retry`). Read `AGENTS.md`/`CLAUDE.md` if present,
`docs/AUTHOR_GUIDE.md` §5.11–5.13, `crates/concord-core/src/{prepared,response,error}.rs`. You
cannot commit; leave the change in the working tree. Network access is enabled for your sandbox
(loopback tests can run); do not contact any external service.

## Ownership (do not blur)

Retry owns: bounded repetition of **application attempts** with explicit replay permission, an
SDK-supplied outcome classification, attempt limits (counting the first attempt), one monotonic
total deadline covering waits, overflow-safe capped exponential backoff with injected jitter,
injectable clock and sleep, and an attempt report. It does **not** own: credentials, rate
admission, pagination, response decoding, provider error schemas. It must not infer permission
from `Clone`, from a 429, or from a method name alone; it must never buffer a one-shot body to
manufacture replayability; it never restarts a stream after items were delivered (the retry
boundary is "final response head received").

Three distinct facts: bytes can be recreated (`reqwest::Request::try_clone()` is `Some`); repeating
is semantically permitted (caller/SDK says so: safe method, provider idempotency key, provider
contract); this outcome warrants another attempt (classifier). Keep them separate in the API.

## What to build

1. `crates/concord-retry` (`publish = false` like the others, workspace version/lints/docs). No
   async runtime dependency in the engine: sleep is injected (`impl Fn(Duration) -> impl Future`
   or a small trait), clock is injected (monotonic `Instant`-like + wall-clock `SystemTime` for
   HTTP-dates). A `tokio` feature may provide the default sleep. Keep the public API small;
   prefer plain functions/closures over traits where they suffice; justify each public type.
2. Engine: e.g. `Retry::new(policy).run(factory, classify)` where `factory(attempt) -> Future<
   Output = R>` performs **exactly one** application attempt and `classify(&R) -> Decision`
   (`Done`, `RetryAfter(Option<Duration>)` with the server's hint, `Stop`). Stop when attempts or
   the deadline would be exceeded; a server minimum delay that exceeds the remaining deadline
   stops (never shortened then retried early). Return the final outcome plus a report: attempts
   started, waits, total waited, why repetition stopped (done, attempts, deadline, classifier
   stop, not replayable). Errors and reports print no secrets.
3. `concord_retry::http`: strict `Retry-After` parsing per RFC 9110 §10.2.3 — delay-seconds or
   HTTP-date (IMF-fixdate; accept the obsolete formats only if you can justify it), distinguishing
   missing, invalid, repeated (several field lines or a comma list → reject; note that dates
   contain commas), overflow, a past date (zero). Convert dates with an injected wall clock. Raw
   value never echoed by `Display`.
4. Concord adapter (feature `concord`, depends on `concord-core`): retry a prepared request:
   take `prepared.into_parts()` once, and for each attempt `request.try_clone()`; if the body
   cannot be cloned and a retry is required, report `NotReplayable` **before any I/O** (and in a
   best-effort mode send once and report replay unavailable); execute with
   `Decoder::execute(&client, request)` so every attempt keeps the endpoint's identity and
   returns an unread `Received<T>` for the SDK's own classification/consumption. Document that a
   caller-supplied `reqwest::Client` may itself retry or redirect: attempt counts are
   application attempts; strict wire accounting needs a client built with
   `retry(reqwest::retry::never())` and an explicit redirect policy (show how in docs). Provide
   an attempt hook (`FnMut(&mut reqwest::Request, attempt)`) so a credential or signature can be
   applied **fresh per attempt after the wait**, and keep a stable idempotency key across attempts
   (it is part of the template; show a test proving the same key on every attempt).
5. Tests (deterministic: injected clock/sleep, no real sleeps or elapsed-time thresholds): limits,
   deadline vs server minimum, jitter bounds, overflow (huge attempt numbers), seconds/date/
   invalid/repeated/past/overflow `Retry-After`, classifier stop, one factory call per attempt.
   Loopback tests with `examples/testserver` (`TestServer::start(|r| Reply ...)`) through a real
   generated endpoint: 503 then 200; 429 with `Retry-After: 1` honored through the injected sleep;
   a streaming non-cloneable body refused before I/O; the attempt hook sees a fresh value per
   attempt; the idempotency header identical on every attempt.
6. Adoption (existing consumers outside other active work): replace `examples/native/src/
   policies.rs::retry_bounded`'s hand-written loop with the crate if that keeps its tests'
   assertions unchanged; use the parser in `examples/riot` for its `Retry-After` projection if it
   keeps behavior. Do **not** edit the Slack, Dropbox, GitHub or GitLab SDKs now (another task is
   editing them); instead list exactly what their migration would change.
7. Gate: add the crate to `scripts/verify.sh` (tests per feature, doc with `-D warnings`),
   `scripts/check-graph.py` (allowed edges: no reqwest/concord-core without the `concord`
   feature), `scripts/check-ci.py`/`package-check.sh`/`consumers.sh` crate lists, and a packaged
   consumer `checks/consumers/retry-only` that uses the engine without Concord's macro crate.

Bounded verification (`CARGO_BUILD_JOBS=4`, one cargo job at a time): the crate's tests per
feature, clippy `-D warnings`, rustdoc `-D warnings`, the adopted examples' tests,
`python3 -I scripts/check-graph.py`. Report the public API with a one-line justification per
item, the decisions you made on ambiguous points, test counts, and what you could not do.
