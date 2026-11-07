# concord-retry

One bounded owner of **application attempts**, with explicit replay permission. No runtime or
HTTP dependency by default. The engine schedules plain futures; `concord` adds a prepared-request
adapter, and `tokio` adds `Retry::run_tokio`. Neither feature enables the other. The engine owns
no credentials, admission, pagination, decoding, provider schemas or stream resumption.

```rust
use concord_retry::{Decision, Policy, Replay, Retry};
use std::{num::NonZeroUsize, time::Duration};

async fn example() {
    let policy = Policy {
        timeout: Some(Duration::from_secs(30)),
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(2),
        ..Policy::new(NonZeroUsize::new(3).unwrap())
    };
    let epoch = std::time::Instant::now();
    // Supply sleep from your runtime. This example uses the optional `tokio` feature.
    #[cfg(feature = "tokio")]
    let run = Retry::new(policy, Replay::Permitted).run(
        |attempt| async move { attempt }, // exactly one application attempt
        |n| if *n == 1 { Decision::RetryAfter(None) } else { Decision::Done },
        || epoch.elapsed(),
        tokio::time::sleep,
        |_, ceiling| ceiling, // deterministic identity jitter; inject RNG-based jitter if wanted
    ).await;
}
```

Three independent facts are required: a factory/template can recreate bytes; the caller/SDK
permits repetition under a safe-operation or provider idempotency contract (`Replay`); the SDK
classifies this particular outcome as retryable (`Decision`). A `Clone` implementation, method
name or 429 response establishes neither of the other facts. Factories must not hide another
retry loop, or application counts will multiply. An idempotency key belongs in the request
template and must remain the same across attempts.

With `concord`, use the generated endpoint's `prepare()` once:

```rust,ignore
let prepared = api.create(item).prepare()?; // idempotency key already in this template
let run = Retry::new(policy, Replay::Permitted).receive(
    prepared,
    concord_retry::concord::ReplayMode::Required,
    |request, attempt| {
        // Called after the wait, on a fresh template clone; refresh a token/signature here.
        // Keep the operation, body semantics and idempotency key stable.
        apply_current_credentials(request, attempt);
    },
    |outcome| match outcome {
        Ok(received) if sdk_warrants_retry(received.head()) => {
            let hint = concord_retry::http::retry_after(
                received.head().headers().get_all("retry-after").iter().map(|v| v.as_bytes()),
                wall_clock(), // SystemTime snapshot; not the monotonic deadline clock
            ).ok().flatten(); // SDK chooses how invalid server hints affect classification
            Decision::RetryAfter(hint)
        }
        _ => Decision::Done,
    },
    monotonic_clock,
    injected_sleep,
    injected_jitter,
).await;
// Check run.report; then consume run.outcome's unread Received<T> in the SDK.
```

For asynchronous credentials, fallible admission or a guard that can expire during backoff,
use the opt-in `Retry::receive_with`. `receive` and `run` keep their existing signatures and
accounting. The hook owns a fresh `reqwest::Request` and returns a future yielding
`Result<(reqwest::Request, A), E>`; neither `A` nor `E` needs an error, clone or debug trait.
The classifier receives `(&Result<Received<T>, concord_core::Error>, &mut A) -> Decision`.
An owned request lets the future await acquisition without boxing or borrowing the hook.

```rust,ignore
let run = Retry::new(policy, Replay::Permitted).receive_with(
    prepared,
    concord_retry::concord::ReplayMode::Required,
    |mut request, attempt| async move {
        let token = tokens.acquire().await?; // SDK-owned acquisition error E
        let lease = limiter.lease().await?;
        request.headers_mut().insert(AUTHORIZATION, token.header().clone());
        Ok((request, (token.generation(), lease))) // per-attempt state A
    },
    |outcome, (generation, _lease)| classify(outcome, generation),
    monotonic_clock,
    injected_sleep,
    injected_jitter,
).await;
// run.refusal is the SDK's E; it never overwrites run.outcome.
// Hold run.context while consuming the final body, then drop it to release the lease.
```

`ReceiveRun<T, A, E>` returns `outcome`, `context`, `refusal` and `report: ReceiveReport`.
A hook error skips dispatch and classification and stops with `ClassifierStop`. The unread
last completed outcome stays available beside that error; on a first-hook refusal it is absent.
`ReceiveReport::retry` is a `Report` whose `attempts` count **dispatches to Decoder::execute**,
including a dispatch cancelled at the deadline, but excluding hooks. `completed` counts returned
heads/transport errors; `retry.attempts - completed` detects a cancelled dispatch. `refused`
counts hook errors (zero or one); a cancelled hook is neither a dispatch nor a refusal.
`no_response` remembers whether **any** dispatch lacked a response head, including cancellation,
even if a later attempt returned a head. Hook failure/cancellation and body decoding do not set
it. This records transport history; the SDK still interprets provider ambiguity such as a 5xx.

The total deadline includes hooks. A successful hook followed by expiry cannot dispatch; a
pending hook is dropped at expiry. At exact expiry `Deadline` takes precedence over refusal,
while the refusal value and previous outcome remain available. The hook's state stays alive
through classification and final decoding. Before an actual retry (including zero backoff),
it is dropped **before constructing sleep or calling the next hook**. Thus a concurrency lease
never spans backoff. If repetition stops at a count/replay/time bound, final state is retained.
If a later hook refuses or expires, the previous outcome remains but its state was released.
Dropping the adapter future cancels owned work; shared acquisition work started by the hook
may outlive it. Required replay preflight runs before hooks and I/O just as with `receive`.
Diagnostics withhold outcomes, context and refusal values, even if their `Debug` leaks secrets.

Each attempt executes `Decoder::execute(&client, request)`, keeping endpoint identity and declared
handling. Dropped retry responses are never decoded or buffered. Once the final head is returned,
body consumption is outside the retry owner: a decode failure or a stream failure after an item
cannot restart the request.

A caller-supplied reqwest client may retry or redirect internally. Reported attempts count
application executions. Strict wire accounting needs an explicitly configured client, as well as
one-attempt factories/hooks:

```rust,ignore
let client = reqwest::Client::builder()
    .retry(reqwest::retry::never())
    .redirect(reqwest::redirect::Policy::none()) // choose an explicit redirect policy
    .build()?;
```

`Required` rejects a non-cloneable body **before hooks, body polling, connections or other I/O**
when multiple attempts are permitted and requested. This conservative preflight happens before
knowing whether the first outcome would need retrying. A single-attempt policy or `Replay::Never`
can send that body once. `BestEffort` sends once, sets `report.replay_available = false`, and stops
with `NotReplayable` if the SDK asks to repeat. If that single outcome is done, the reason is
`Done` and replay availability is still false. Nothing reads a one-shot body to make it cloneable.

The total deadline starts inside `run`, includes attempts, classification and backoff, and is
never reset. Time spent preparing the template before `receive` is outside it. Pending attempt
futures are dropped on expiry; cancellation cannot undo a server-side effect. At exact expiry,
`Deadline` takes precedence. Otherwise Done/Stop take precedence over replay and count checks;
for a retry decision unavailable replay takes precedence over the count limit. `outcome` is the
last completed outcome, including when a later attempt expires; `None` means none completed.
Synchronous blocking work cannot be preempted by an async timer.

Backoff after attempt n is `min(initial * 2^(n-1), cap)`, with saturating arithmetic even for
`usize::MAX`. Jitter receives that ceiling and is clamped into `[0, ceiling]`. The server delay
is a **minimum** applied after jitter and can exceed the local cap. A delay equal to or above the
remaining budget stops immediately; it is never shortened to squeeze in another attempt. The
clock must be monotonic; sleep futures must be lazy, cancellation safe, and honor the requested
minimum. Injected sleeps also implement deadline timers for pending attempts. Report `waits`
contains requested, nonzero backoff delays; `total_waited` measures actual monotonic wait time,
including oversleep. Deadline timer durations are excluded from both.

`http::retry_after` takes **all** field lines as byte slices, an injected `SystemTime`, and returns
`Result<Option<Duration>, RetryAfterError>`. Missing differs from invalid, repeated and overflow;
a past date is zero. HTTP OWS (SP/HTAB), unsigned decimal seconds, and IMF-fixdate are supported.
Calendar dates, weekday agreement, GMT and fixed widths are validated, including dates before
1970. Obsolete formats are rejected deliberately: no two-digit-year ambiguity or time-dependent
century heuristic. The parser treats the single comma following a weekday as date syntax;
additional commas or non-date comma lists are rejected as repeated. Malformed date-like input is
invalid. It preserves fractional wall-clock differences instead of rounding a minimum down.
Errors store no raw bytes, and `Run`'s Debug withholds even a secret-printing outcome.

Public API, with the reason each item exists:

| Item | Why it is public |
| --- | --- |
| `Policy` (`new`, `backoff`) | Bounds and inspectable overflow-safe local scheduling. |
| `Replay` | Semantic permission independent of byte recreation and classification. |
| `Decision` | The SDK's head/transport outcome classification and optional server minimum. |
| `Retry` (`new`, `run`, optional `run_tokio`, `receive`, `receive_with`) | A single owner of the attempt loop and total deadline; closures supply all dependencies. |
| `Run<R>` | Keeps the final unconsumed outcome beside accounting, including preflight/timeout absence. |
| `Report` | Exposes actual started attempts, waits, stop reason and replay availability without values. |
| `StopReason` | Makes exhaustion, deadline, caller stop and replay refusal distinguishable. |
| `http::retry_after` | Shared strict singleton-header parsing without a transport dependency. |
| `http::RetryAfterError` | Distinguishes invalid, repeated and overflowing fields without echoing values. |
| `concord::ReceiveRun<T, A, E>` | Preserves a typed hook refusal beside the last head/error and final admission state. |
| `concord::ReceiveReport` | Separates dispatches, completions and refusals; remembers headless dispatches without changing `Report`. |
| `concord::ReplayMode` | Makes preflight refusal versus best-effort single sending explicit. |

Adoption: native's `retry_bounded` now delegates to the engine, retaining its existing signature,
at-least-once behavior, linear schedule and caller-owned error predicate. Riot uses the shared
parser while retaining its numeric-seconds-only projection and stop-on-429 behavior; dates remain
absent there, and repeated fields are rejected. No Slack, Dropbox, GitHub or GitLab source was
changed. Their concrete migration would be:

- Slack `src/error.rs::ResponseMeta::retry_after`: pass all field lines to the parser, keep the
  SDK's Missing/Seconds/Invalid projection, mapping both Repeated and Overflow to Invalid. Keep
  Slack's documented numeric-only contract by rejecting dates before projection. Retain the
  current send policy's HTTP-versus-envelope handling; no automatic retry is implied.
- Dropbox `src/error.rs::ResponseMeta::retry_after`: the same singleton parser projection; keep
  its provider error document's separate `retry_after` field and tagged error decoding in the SDK.
  No upload replay can be enabled just because its metadata contains a delay.
- GitHub `src/meta.rs::ResponseMeta::from_head` (used by `capture`): replace only `number(&RETRY_AFTER)` with shared
  singleton parsing, projecting numeric seconds into the existing `Option<u64>`; retain all
  other numeric rate headers and the SDK's `is_rate_limited` interpretation. HTTP-date support
  would require an explicit wall clock and documented change to the existing projection.
- GitLab `src/meta.rs::ResponseMeta::from_head` (used by `capture`): replace the local `Retry-After` integer projection
  with the same singleton numeric projection; retain pagination, request IDs and provider errors.
  Its documented numeric-only public accessor need not change.

If any of those SDKs later opts into repetition, its SDK policy must explicitly select permission
per operation (and stable provider idempotency protection for writes), classify unread heads and
transport outcomes, choose count/deadline/backoff, and execute one prepared-request adapter.
Move rotating credentials/signatures to the hook. Keep rate admission outside, never restart
streams, and preserve existing provider error consumption on the final outcome. These are opt-in
behavior changes, not part of this migration.

License: MIT OR Apache-2.0.
