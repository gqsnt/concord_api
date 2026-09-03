# Concord v2 runtime model

- **Audience:** callers, operators, extension authors, and maintainers
- **Status:** normative for execution and runtime configuration

The [DSL specification](spec.md) owns declarations. This page defines what
happens after a generated or runtime endpoint is bound to a built client.

## Immutable client runtime

A successful build freezes an application HTTP configuration and a runtime
configuration. Client clones share transport, security identity, versioned
credential slots, limiter, retry/budget handles, gates, completion observer,
diagnostic sink, and finite limits. Scope and endpoint snapshots retain their
own immutable domain values. Independent builds have independent identities
unless the application deliberately supplies shared service handles.

`HttpConfigBuilder` owns TLS, proxy, protocol, connection, and transport
timeouts. `RuntimeConfigBuilder` owns request/response/stream/record/provider/
pagination limits, retry, limiter, gates, completion, and diagnostics. Trust
choices use intention-revealing methods such as `require_https()` and
`allow_http()`.

A client whose contract has an effective rate policy fails construction unless
a limiter was selected. No production allow-all bypass is installed.

## Laziness and one executor

Building clients, selecting scopes, constructing endpoints, creating unbound
intents, binding them, describing them, and planning them perform no managed
effects. An execution terminal allocates operation identity and begins work only
when its future is first polled.

Direct generated calls, unbound endpoint intents after binding, runtime-defined
endpoints, endpoint-backed credentials, retries,
authentication recovery, pages, response metadata, and accepted bytes all enter
one executor. There is no native request/response callback or alternate HTTP
path.

## Operation order

For each actual attempt, core preserves this authority order:

```text
credential-free route/query/header shaping
request decision gate
body/prerequisite preparation as needed
credential-context validation and private material resolution
rate-limit admission
private credential placement
native dispatch
challenge attribution and at most one admitted recovery
response decision gate and declared status handling
sealed response acquisition
conversion
fixed result
completion observation
```

Implementation may avoid expensive body materialization until send is viable,
but missing/foreign credential material fails before quota admission and private
placement happens after admission. Every resend reacquires admission.

## Public and private request views

The executor maintains a real logical public head and a redaction-aware observed
head. Private authentication is applied only to a native attempt copy. Ordinary
plans, gates, classifiers, limiter metadata, completion, errors, and debug output
never contain secrets, credentials, body values, or native requests.

Visible ordinary public values may be sensitive. A declared redacted value uses
real bytes on the wire and for internal pagination/rate identity but becomes a
fixed marker in every ordinary structured view. Concatenation propagates
redaction to the entire atom.

## One absolute deadline

One terminal invocation creates at most one absolute operation deadline. It
covers gates, synchronous extension checks, body preparation/materialization,
credential acquisition/recovery, limiter wait, connection/send/receive, retry
backoff, every attempt, acquisition, conversion, pagination controller work,
every page, and aggregate completion.

Pagination never creates a fresh deadline per page. Core checks before and after
trusted synchronous callbacks. It cannot preempt synchronous application code,
so such callbacks must remain bounded and nonblocking.

With timeout disabled, extension futures can remain pending; that is an explicit
application choice. Transport-specific timeouts remain additional HTTP policy,
not operation-deadline replacements.

## Attempts, stages, and dispatch knowledge

Operation, attempt, page, and pagination-aggregate identities are separate.
Execution tracks at least preparation, before dispatch, dispatch/unknown,
rejected response, response acquisition, and conversion stages.

Caller-visible dispatch knowledge is:

- `NotDispatched`: core conclusively failed before native dispatch;
- `PossiblyDispatched`: dispatch began but no response head proves receipt;
- `ResponseReceived`: a response head arrived, even if status, acquisition, or
  conversion later failed.

None proves whether remote side effects committed or rolled back.

## Retry gates

An application retry occurs only when all relevant gates agree:

```text
configured policy and classifier recommendation
eligible failure stage
semantic endpoint repeatability when dispatch may have occurred
mechanically reconstructable/replayable body state
per-operation attempt maximum
shared retry budget
remaining aggregate deadline
```

The classifier advises and cannot bypass core gates. Conversion failures do not
retry. Acquisition or rejected transient-response failures may retry only for
repeatable/replayable operations. A nonrepeatable endpoint may retry only after
a conclusive pre-dispatch failure with reconstructable body state.

Every retry receives a new attempt identity and rate admission. A received
response still does not prove application side-effect state.

## Authentication and recovery

Credential slots are versioned and single-flight. Stale acquisition/recovery
cannot overwrite newer material. Provider HTTP is isolated, bounded, deadline
limited, and does not inherit application client identity PEM. Endpoint-backed
credential work executes through this same managed executor.

Recovery requires an explicit signal, attributed challenge, replayable body,
remaining attempt/deadline bounds, and new rate admission. It occurs at most once
per request/page sub-operation and is not counted as an application retry.
Different pagination pages may each recover, but the aggregate remains bounded.

The recovery declaration assumes that the service's challenge makes one resend
semantically acceptable. Services violating that assumption are incompatible
with automatic recovery.

## Rate admission

Core resolves a policy plan and canonical keys before calling `RateLimiter`.
The limiter decides admit/wait/reject and owns its algorithm, storage, and
coordination. It cannot alter endpoint semantics or observe credentials/native
requests. Key values are intentionally opaque to ordinary plans and evidence.

Admission occurs once per actual network attempt, including policy retry and
authentication recovery, under the same operation deadline. Response-head
observation may update future limiter state but cannot replace the received
response.

## Response authority

`ResponseSpec` owns accepted status semantics, default `Accept`, one sealed mode,
and conversion from the semantic accepted head plus acquired representation.
Core owns challenge precedence, native polling, decompression, response-head and
body bounds, cancellation, classification, and orchestration.

The modes are:

- `Buffered`: complete bytes retained under a configured bound before conversion;
- `Streaming`: a lazy bounded `ByteStream` whose later polling remains managed;
- `Discard`: accepted body intentionally not polled.

`.response().await` adds accepted metadata to the declared output.
Endpoint-declared `.accepted_bytes().await` retains authentication, acceptance,
negotiation, and bounds while skipping conversion. Discard may explicitly switch
to bounded buffering; streaming cannot expose this terminal.

Opening a stream fixes a successful stream-open result, not complete body
consumption. Later stream errors occur while polling the stream. Dropping it
stops further local acquisition.

## Pagination

Pagination is sequential. `Prefix(0)` returns an empty result immediately with
no body/provider/limiter/gate/observer/HTTP work. `Prefix(n)` returns at most `n`
successfully collected pages even when continuation remains.
`CompleteWithin(n)` requires natural completion by page `n`; otherwise it
returns a limit error and no partial aggregate.

Any page, controller, cycle, bound, retry, recovery, or deadline failure is
all-or-error. Independent bounds cover pages, flattened items, cumulative
accepted bytes, cycle identities, controller state, attempts, and deadline.
The controller may update only generated endpoint slots named by live bindings.

Cycle identity uses the real credential-free public request (including real
redacted fields) while diagnostics use observed markers. Credentials never
participate. A paginated body must explicitly prove page stability.

## Gates and completion observation

`RequestGate::before_send` and `ResponseGate::before_response_handling` are
result-determining authority at named pre-outcome phases. They receive only a
bounded credential-free/redacted lifecycle view and can reject with a policy
category. They cannot mutate native/private state.

`CompletionObserver` receives exactly one fixed bounded event for the documented
terminal. It returns no decision and cannot turn success into timeout or another
error. Its duration may be diagnosed, but the caller result is already fixed.
Core does not spawn unbounded detached export tasks; asynchronous export belongs
in an explicitly bounded adapter with owned shutdown/backpressure policy.

## Cancellation truth

Dropping a request future stops future local polling and hidden work is not
spawned to continue it. If dispatch may have occurred, cancellation does not
prove the server stopped work or rolled back. The final available error context
uses dispatch knowledge rather than stronger claims.

## Errors and trusted diagnostics

Public errors expose stable caller-oriented categories: configuration,
credentials/authentication, rate admission, policy rejection, timeout,
transport, rejected status, acquisition, conversion, pagination, and internal
invariant failure. Safe structured context includes endpoint/method, operation,
attempt/page identities, stage, and dispatch knowledge where applicable.

Ordinary `Display`/`Debug` excludes credentials, body content, raw dependency
errors, and redacted public values. Source chains are retained through a trusted
diagnostic channel. A trusted sink is separate from gates and ordinary
observation and must still protect application public data. All retained
diagnostic strings, labels, histories, and queues are bounded.

## Runtime-defined endpoints

Runtime endpoints retain first-error latching and finite structural budgets.
Their builder uses validated subdomain/path operations, ordered query
assignment/append/removal/off, header replacement/removal/off, grouped auth
addition/replacement/off, response-owned accept/off, explicit `Repeatability`,
body/response specifications, canonical rate policies, and explicit accepted
bytes selection.

They bind to one generated client security/runtime context and use the same head
builder, planner, executor, errors, limits, and observations. They expose no
generic mutation of generated client/scope values and no native request escape.

## Extension trust

Core extension traits each vary one decision: body encode/preparation, response
conversion, record framing, credential acquisition, pagination continuation,
rate admission, retry recommendation, gate rejection, or completion recording.
Extensions own boundedness of application work and retained state not visible to
core. No trait may override ordering, private placement, semantic repeatability,
deadline, acquisition mode, or native transport ownership.
