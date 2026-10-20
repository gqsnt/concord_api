# Execution policy: controls, time and truthful outcomes

Every generated, unbound, runtime-bound, accepted-byte, storage, stream and
traversal request enters one managed executor. This page covers what a caller
can control per request (§8) and what the executor guarantees about each
operation and its errors (§13).

## Request controls

Every bound request and configurable terminal retains three things
(CONTROL-01): its **effective declared timeout**, its **selected timeout** and
independent additional **caps**. A selected timeout is a preference; a cap is a
constraint.

```text
request.timeout(duration) -> Result<Self, ControlError>
request.reset_timeout() -> Self
request.cancel_with(token) -> Result<Self, ControlError>
request.with_options(&options) -> Self
request.control_summary() -> ControlSummary
```

These are inherent on generated requests and on the configurable terminal
wrappers `ResponseRequest`, `AcceptedBytesRequest`, `StoreRequest`,
`CollectPrefix` and `CollectAll`. `cancel_with`, `with_options` and
`control_summary` are also the open `RequestControls` trait. An already
constructed `Pages` iterator and an acquired stream carry the controls captured
from the request and expose no setters: apply controls before calling
`.pages()` or awaiting a stream.

- `timeout(d)` replaces the selected finite duration; `reset_timeout()`
  restores the declared (or 30-second default) duration. Neither removes a cap.
- A zero or unrepresentable duration is a `ControlError`
  (`ZeroDuration`, `UnrepresentableDuration`) before anything executes.

### Caps with `CallOptions`

```text
CallOptions::new()                                   // == CallOptions::default(): no additional caps
    .cap_timeout(Duration) -> Result<Self, ControlError>
    .cap_sends(u32)
    .cap_replays(u32)
    .cap_response_bytes(NonZeroUsize)
    .cap_stream_lifetime(Duration) -> Result<Self, ControlError>
    .cap_stream_idle(Duration) -> Result<Self, ControlError>
```

Repeated setters intersect; `with_options` takes the componentwise minimum of
every present cap. A missing cap means no *additional* restriction, not
unbounded execution. Caps propagate through `.response()`,
`.accepted_bytes()`, `.store_as_*()`, `.pages()` and both collection terminals;
transforming a request never drops a restriction. A 5-second cap followed by
`timeout(60s)` still ends the operation at 5 seconds.

- `cap_sends(n)` counts every application attempt handoff, including the first
  submission of each later page. Zero is valid and prevents dispatch without
  running body or credential preparation first.
- `cap_replays(n)` counts extra automatic submissions from policy retry or auth
  recovery, cumulatively across a traversal.
- Caps never grant a retry that effect, body or authority rules disallow.

`ControlSummary` reports the declared, selected and effective timeout
(`declared_timeout()`, `selected_timeout()`, `effective_timeout()`), each cap and
the number of cancellation sources, with no token internals or request values.

### Cancellation

`CancellationToken::new()` is a cheap cloneable handle with `cancel()`,
`is_cancelled()` and an awaitable `cancelled()`. Clones share one signal and it
never resets. A request combines up to eight distinct signals by OR; re-adding
the same signal deduplicates, and a ninth returns
`ControlError::TooManyCancellationSources` rather than dropping a restriction.

Unpolled requests start nothing, reserve no storage intent and emit no events
(CONTROL-03). A token observed before first dispatch prevents it. Dropping a
polled future cancels owned local work. Cancellation cannot retract a request
already handed to the transport, undo a remote effect, refund committed quota or
erase contact evidence. Page fetches and stream reads have deliberately
different drop rules ([Pagination](pagination.md), [Streams](streams.md)).

## One absolute deadline

The deadline begins at the first terminal poll and covers public rendering,
body preparation, credential and rate waits, every attempt, retry sleep,
acquisition, conversion and controller callbacks. It is never restarted for
another attempt or page. Time is checked before and after synchronous extension
calls, but synchronous Rust cannot be preempted: a blocking callback delays the
result. Final observation runs after the result is fixed and can delay delivery
beyond the nominal deadline without changing the result (EXEC-04). For
`.store_as_*()`, local credential installation belongs to the result: the
deadline and cancellation are decided immediately before installing, and the
final observation follows the installed, superseded or failed storage outcome
([Authentication](auth.md#explicit-credential-storage)).

## The operation pipeline

| Phase | Behavior |
|---|---|
| 0. First poll | Validate controls and cancellation, allocate the operation identity and deadline, reserve an explicit storage intent if applicable |
| 1. Public selection | Compose the symbolic query/header programs, resolve suppression and outer presence, validate key and target ownership |
| 2. Public rendering | Encode only surviving atoms and protected rate dimensions within bounds |
| 3. Body preparation | Prepare a present body once under the deadline and byte ceiling |
| 4. Credential preflight | Obtain only the declared required material |
| 5. Attempt admission | Enforce send/replay caps and the retry budget; acquire complete conjunctive quota admission |
| 6. Final freshness | Verify applied credential versions are still usable; at most two restarts per attempt |
| 7. Native handoff | Apply private credentials, finish framing, record handoff, send under the remaining deadline |
| 8. Response head | Record `ResponseObserved`, parse the server delay once, send limiter feedback, attribute a challenge |
| 9. Result branch | Permitted recovery or retry, or normal payload / expected absence / rejected-status acquisition |
| 10. Completion | Fix the result, status, contact and counts; observe; return the typed result or a correlated error |

Planning (`plan()`) shares phases 1-2 and never runs phases 3-10: it cannot
trigger a provider, encode a body, acquire a limiter or open a connection
(EXEC-03). Freshness restarts do not reset the deadline or inflate attempt
caps, and no lock is held across I/O.

## Identities and dispatch knowledge

A direct operation, a traversal, a page, a provider acquisition, a visible
attempt and a transport-private physical try are distinct identities (EXEC-01).
Page and attempt indexes are zero-based in machine metadata.

```text
DispatchKnowledge: NotDispatched < PossiblyDispatched < ResponseObserved
```

The operation's aggregate is a monotone join and never regresses. Receiving a
response head records `ResponseObserved` irrevocably; a later body failure or a
later attempt's connect failure cannot clear it. Only positive, core-native
proof of no application submission settles an attempt as `NotDispatched`; a
drop, timeout or ambiguous transport failure settles as `PossiblyDispatched`.
`may_have_reached_server()` means the aggregate is not `NotDispatched`; it does
not prove an effect or success.

## Errors

`ApiError` is value-free and exposes:

| Accessor | Meaning |
|---|---|
| `category()` | stable `ErrorCategory` (`Configuration`, `RequestEncoding`, `RequestBody`, `AuthenticationUnavailable`, `AuthenticationProvider`, `AuthenticationRejected`, `RateLimit`, `Timeout`, `Connect`, `Transport`, `HttpStatus`, `ResponseBody`, `ResponseDecoding`, `ResponseContract`, `Pagination`, ...) |
| `failure_stage()` | `FailureStage`: `Preparation`, `CredentialPreflight`, `RateAdmission`, `BeforeDispatch`, `DuringDispatchOrUnknown`, `RejectedResponse`, `ResponseAcquisition`, `ResponseConversion`, `PaginationDecision`, `CredentialInstall`, `StreamConsumption` |
| `endpoint_id()`, `method()` | the validated operational endpoint ID and method |
| `operation_id()`, `parent_operation_id()` | operation identities, when an operation started |
| `dispatch_knowledge()`, `may_have_reached_server()` | aggregate dispatch knowledge |
| `last_attempt()`, `operation_counts()` | last-attempt facts and bounded counts, separately from the aggregate |
| `http_status()`, `last_response_status()`, `http_response()` | status facts and the rejected-status body ([Responses](responses.md)) |
| `auth_kind()`, `rate_limit_kind()`, `pagination_kind()`, `configuration_kind()` | category-specific kinds |
| `traversal_summary()`, `stream_summary()` | the traversal or stream summary, when applicable |
| `retry_after()` | the parsed server delay, when one was observed |

Response-chunk errors, timeouts after a head, decode failures, controller
rejections and failed credential installation all keep the current dispatch
knowledge and status (EXEC-02). The last attempt can be definitively unsent
while an earlier attempt was observed. `ControlError` and `ClientBuildError`
occur before any operation and carry no fabricated operation ID. Displays and
`source()` chains never contain raw URLs, headers, bodies or decoder messages.

## Reconciling failed writes

For a failed write, `ResponseObserved` or `PossiblyDispatched` means the
application may need reconciliation with its durable command ID; it does not
mean "retryable". Even an observed 2xx followed by a body failure can have
committed the write. The canonical example keeps the command ID, never
resubmits under a new one, and formats its own wrapper types without printing
the ID or payload:

<!-- concord-source: crates/concord-examples/src/application/reconciliation.rs#reconciliation -->
```rust
use std::fmt;

use concord::{ApiError, CallOptions, DispatchKnowledge};

use super::execute::execute;
use crate::contracts::catalog_v3::{Item, UpdateInput, catalog_api};

/// A durable application command identifier (for example persisted before
/// the first attempt). Formatting never prints the value.
#[derive(Clone, Eq, PartialEq)]
pub struct CommandId(String);

impl CommandId {
    /// Wraps an application-generated durable identifier.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicitly exposes the identifier for reconciliation or storage.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for CommandId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CommandId(<redacted>)")
    }
}

/// One durable title change: the command identifier and its payload.
pub struct TitleChange {
    /// Target item.
    pub item_id: String,
    /// Durable command identifier sent as `Idempotency-Key`.
    pub command_id: CommandId,
    /// New title (application data; not formatted automatically).
    pub title: String,
}

impl fmt::Debug for TitleChange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TitleChange")
            .field("item_id", &"<redacted>")
            .field("command_id", &self.command_id)
            .field("title", &"<redacted>")
            .finish()
    }
}

/// Why a write did not return a confirmed item.
pub enum WriteFailure {
    /// Concord settled that the application request was not dispatched. The
    /// same command may be submitted again later, with the same identifier.
    NotDispatched {
        /// The retained durable identifier.
        command_id: CommandId,
        /// The value-free library error.
        error: ApiError,
    },
    /// The request may have reached the provider (possibly dispatched or a
    /// response was observed). Reconcile using the retained identifier; do not
    /// resubmit under a new one.
    OutcomeUnknown {
        /// The retained durable identifier.
        command_id: CommandId,
        /// The value-free library error.
        error: ApiError,
    },
}

impl WriteFailure {
    /// Returns the retained durable command identifier.
    #[must_use]
    pub fn command_id(&self) -> &CommandId {
        match self {
            Self::NotDispatched { command_id, .. } | Self::OutcomeUnknown { command_id, .. } => {
                command_id
            }
        }
    }

    /// Returns the underlying value-free library error.
    #[must_use]
    pub fn error(&self) -> &ApiError {
        match self {
            Self::NotDispatched { error, .. } | Self::OutcomeUnknown { error, .. } => error,
        }
    }

    /// Whether explicit reconciliation is required before any new attempt.
    #[must_use]
    pub fn needs_reconciliation(&self) -> bool {
        matches!(self, Self::OutcomeUnknown { .. })
    }

    fn classification(&self) -> &'static str {
        match self {
            Self::NotDispatched { .. } => "NotDispatched",
            Self::OutcomeUnknown { .. } => "OutcomeUnknown",
        }
    }
}

impl fmt::Debug for WriteFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = self.error();
        formatter
            .debug_struct("WriteFailure")
            .field("classification", &self.classification())
            .field("command_id", self.command_id())
            .field("dispatch", &error.dispatch_knowledge())
            .field("stage", &error.failure_stage())
            .field("status", &error.last_response_status())
            .finish()
    }
}

impl fmt::Display for WriteFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error = self.error();
        write!(
            formatter,
            "title change {} ({:?}, stage {:?}); command identifier retained for reconciliation",
            self.classification(),
            error.dispatch_knowledge(),
            error.failure_stage(),
        )
    }
}

impl std::error::Error for WriteFailure {}

/// Classifies a failed write by the journal's aggregate dispatch knowledge.
#[must_use]
pub fn classify(command_id: CommandId, error: ApiError) -> WriteFailure {
    match error.dispatch_knowledge() {
        DispatchKnowledge::NotDispatched => WriteFailure::NotDispatched { command_id, error },
        _ => WriteFailure::OutcomeUnknown { command_id, error },
    }
}

/// Submits one title change exactly once under the caller's limits.
///
/// # Errors
///
/// A [`WriteFailure`] that keeps the durable command identifier.
pub async fn change_title(
    scope: &catalog_api::regional::Regional,
    change: TitleChange,
    limits: &CallOptions,
) -> Result<Item, WriteFailure> {
    let TitleChange {
        item_id,
        command_id,
        title,
    } = change;
    let request = scope.update_item(item_id, command_id.expose(), UpdateInput { title });
    execute(request, limits)
        .await
        .map_err(|error| classify(command_id, error))
}
```

Concord's errors are value-free; application wrapper types are not
automatically so. Implement `Debug`/`Display` deliberately for types that carry
identifiers or payloads, as above.
