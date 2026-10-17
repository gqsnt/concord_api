# Planning, inspection and observation

Concord generates four distinct views from the resolved contract and the
execution journal (EXPLAIN-01). Each answers a different question and must not
be mistaken for another:

| View | Answers | Must not imply |
|---|---|---|
| Method/hover rustdoc | What to call, required inputs, result, capabilities, inherited auth and timeout | That runtime values or provider behavior are known |
| Static contract (`Client::contract()`, `description()`) | Effective symbolic behavior, origins, fallbacks, coverage and identities | That a plan was executed or opaque Rust code was proved equivalent |
| Bound `RequestPlan` | This request's public shape and effective controls | Credential acquisition, quota admission, body preparation or a send |
| Execution, traversal and stream outcomes | What happened as of the recorded stage and why it stopped | Remote rollback, complete consumption at acquisition, or business intent |

## Static contracts

`CatalogApi::contract()` returns a `&'static ContractDescription` without an
executable client, secret or network service; a generated request or intent's
`description()` returns its `&'static EndpointDescription`. Building a
description never evaluates payload defaults, invokes converters or encodes
values. The description preserves (§18.2):

- the owner tree, source origins, author docs and deprecation, and local versus
  effective attachments and fences;
- each value's owner, payload type, presence, initializer identity,
  sensitivity, liveness and constructor order;
- each endpoint's method, exact result/body types and selected representation,
  with declared versus inherited provenance;
- the ordered path/DNS template IR and the query/header operation IR
  (literals, value references, adapters, `concat`, `each`, conditional
  branches);
- per-endpoint credential sources, targets, challenge rules and whether
  automatic recovery is actually available;
- explicit and derived endpoint IDs; policy IDs, window dependencies, ordered
  key labels and encoding identities, bindings, cost, coverage and possible
  sharing;
- accepted and absent statuses, effect and its source, timeout selections,
  pagination Init/Cursor mappings, body/driver capabilities and terminal
  availability;
- opaque dependencies: Rust types, constants, adapters, controllers and
  providers whose semantics cannot be proved from declaration text.

A sensitive literal is held only as a redacted marker with its classification
and location; a comparison involving it reports an unresolved opaque
dependency rather than equality.

## Bound plans

<!-- concord-source: crates/concord-examples/src/application/planning.rs#catalog_planning -->
```rust
use std::num::NonZeroU32;

use concord::ApiError;

use crate::contracts::catalog_v3::{CatalogApi, Item, ItemSummary};

/// Inspects public request shapes and selects terminals without executing them.
///
/// # Errors
///
/// Returns a public head-planning error if a declared value cannot be encoded.
pub fn inspect_requests(client: &CatalogApi) -> Result<(), ApiError> {
    let regional = client.regional("eu-west".to_owned());
    let request = regional.item_summaries(Vec::new());
    let _plan = request.plan()?;
    let _complete: concord::pagination::CollectAll<ItemSummary> =
        request.collect_all(NonZeroU32::new(20).expect("positive"));
    let _accepted: concord::endpoint::AcceptedBytesRequest =
        regional.item("item-42".to_owned()).accepted_bytes();

    let endpoint = CatalogApi::unbound()
        .regional("eu-west".to_owned())
        .item("item-42".to_owned());
    let _description = endpoint.description();
    let _unbound_plan = client.plan(&endpoint)?;
    let request = client.request(endpoint);
    let _response: concord::endpoint::ResponseRequest<Item> = request.response();
    Ok(())
}
```

`request.plan()`, `client.plan(&intent)` and `owner.plan_runtime(&endpoint)`
run public selection and rendering only: they evaluate surviving public atom
encoders but never prepare a body, trigger a provider, acquire a limiter,
register a schema or open a connection (EXEC-03). A `RequestPlan` provides:

| Accessor | Fact |
|---|---|
| `observed_url()` | the sanitized logical URL, with sensitive components redacted |
| `public_headers()` | the ordered sanitized public headers |
| `endpoint_id()`, `method()`, `description()` | identity and the static description |
| `timeout_selection()`, `resolved_timeout()` | the effective timeout and its source |
| `body_category()`, `body_description()` | body presence and the static body hint; exact Content-Type and length are deferred to preparation |
| `absent_status()`, `response_acquisition()`, `capabilities()` | absence, driver and terminal facts, including effective authentication |
| `rate_limit()`, `rate_limit_policy_count()` | effective policy IDs and key-label schemas without values, and the enforcement choice |
| `runtime_binding()` | for runtime endpoints: the owner, operations, and authentication disposition |

A plan never displays a placeholder as the actual rate origin, and it labels a
`NoopRateLimiter` as "declared policies; enforcement explicitly disabled by
application selection" rather than "rate limited" (EXPLAIN-04). A runtime plan
includes inherited root and scope attachments, auth and rates, and every
explicit runtime removal; removed authentication is displayed prominently.
Plans and execution are separate preparations, so a plan is not a cache and
does not guarantee later user code cannot fail.

## The `concord.contract/3` format

`concord.contract/3` is the versioned JSON structural export of a
`ContractDescription`, produced by the `concord-inspect` companion crate so
the runtime carries no JSON dependency (§18.3):

- top-level keys `format`, `client`, `owners`, `endpoints`, `policies`,
  `window_sets`, `dependencies` and `diagnostics`;
- every record has a stable structural key (`path`, `id`, `name` or
  `kind` + `reference`); semantic arrays keep declaration order and object keys
  are sorted, so exports are byte-identical across runs;
- only integers and strings: durations are `{ "secs": u64, "nanos": u32 }`;
- no timestamps, absolute paths, runtime values, bodies, material or raw keys;
  sensitive literals appear only as redacted markers.

Two equal snapshots establish only equality of the represented structural
facts. Opaque imports, constants and adapters still need behavior checks, and
IDs are never compared "modulo IDs" because they enter rate state and
correlation.

## Contract tooling

```text
cargo run -p concord-xtask -- contract <export --target NAME --output FILE | check --target NAME --snapshot FILE | diff BEFORE AFTER [--format text|json] | list>
```

| Subcommand | Effect | Exit status |
|---|---|---|
| `export` | writes the target's `concord.contract/3` document | 0, or 2 on usage/I/O/format errors |
| `check` | compares the target with a snapshot, listing changed endpoint IDs/paths and classifications | 0 when structurally equal, 1 on a difference |
| `diff` | a semantic diff of two documents, as text or JSON | 0, or 2 on errors |
| `list` | the registered targets | 0 |

Targets resolve through an explicit, maintained registry: a proc macro cannot
discover every invocation. `fixture-catalog` is always available; the
repository examples (`catalog`, `riot`, `data-dragon`) are
registered with `cargo run -p concord-xtask --features examples -- contract ...`
and exported and checked by `cargo test -p concord-xtask --features examples`.
Export runs no application network work. `check` is not a semantic-versioning
oracle.

`diff` classifies each change on one or more axes: `CallerApi`, `Wire`,
`PresenceAndDefaults`, `AuthAuthority`, `RuntimePolicy`,
`IdentityAndRateSchema`, `SharingCoverage`, `Capability`, `Provenance`,
`Documentation` and `OpaqueDependency`; anything unclassified is reported as
`Unknown`, never as unchanged. It distinguishes changes a lossy snapshot would
miss (EXPLAIN-02): switching a credential source at the same header, changing a
literal without changing the key, remapping a value, removing a representation
fence, changing a default expression, renaming a rate label, and splitting or
merging policy sharing. Rate changes show the covered endpoints, possible
sharing and cost/window edits separately (RATE-02).

Outside this repository, call the same library on your own client from a small
Rust tool:

```text
let json = concord_inspect::export_string(TheirClient::contract());
let report = concord_inspect::check(TheirClient::contract(), &snapshot)?;
if !report.is_structurally_equal() { /* fail the build */ }
```

`concord_inspect::explain_endpoint` and `explain_policy` render the expanded
text views (see the [Catalog example](modeling_apis.md#the-effective-view-of-one-endpoint)).

## Observing operations

An `OperationObserver` receives bounded, sanitized, borrowed facts. It is
read-only and synchronous: it cannot mutate, veto, retry or start asynchronous
work through core.

<!-- concord-source: crates/concord-examples/src/runtime_policy/hooks.rs#hooks_implementation -->
```rust
#[derive(Default)]
pub struct CountingHooks {
    attempts: AtomicUsize,
    responses: AtomicUsize,
    completions: AtomicUsize,
}

impl CountingHooks {
    /// Returns the number of before-send callbacks observed so far.
    #[must_use]
    pub fn attempts(&self) -> usize {
        self.attempts.load(Ordering::Relaxed)
    }
}

impl OperationObserver for CountingHooks {
    fn observe(&self, event: OperationEvent<'_>) {
        match event {
            OperationEvent::AttemptStarted(_) => {
                self.attempts.fetch_add(1, Ordering::Relaxed);
            }
            OperationEvent::ResponseHeadObserved(_) => {
                self.responses.fetch_add(1, Ordering::Relaxed);
            }
            OperationEvent::OperationFinished(_) | OperationEvent::PaginationFinished(_) => {
                self.completions.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}
```

<!-- concord-source: crates/concord-examples/src/runtime_policy/hooks.rs#hooks_client -->
```rust
pub fn construct() -> Result<ObservedApi, concord::ClientBuildError> {
    let observation = Arc::new(CountingHooks::default());
    ObservedApi::builder()
        .configure_runtime(move |runtime| {
            runtime
                .operation_observer(observation)
                .debug(DebugLevel::Basic)
        })
        .build()
}
```

`OperationEvent` is `AttemptStarted`, `ResponseHeadObserved`,
`OperationFinished`, `ProviderAcquisitionFinished`, `PaginationFinished` or
`StreamFinished`. Events carry
operation and attempt identities, value-free target occurrence counts, typed
response facts and outcomes. They expose no exact URL, path or query values,
header names or values, bodies, credential material or raw rate keys. Explicit
`RequestPlan` and `ApiResponse` accessors provide the separate caller-selected
public URL/header view described above. Final
events run after the result is fixed and cannot change it; a slow final
callback can delay delivery. Observer panics are contained where Rust unwinds
and counted only in bounded private diagnostics; no detached observer thread or
queue exists (EXEC-04).

`DebugLevel::Basic` emits structural request/response metadata without headers;
`DebugLevel::Headers` adds value-free request-target counts and typed response
facts. Header names and values are never included at any level.

## Execution outcomes

Results and errors carry the fourth view: `ExecutionMeta` on successful
responses and pages, `TraversalSummary` on traversals, `StreamSummary` on
streams and the value-free `ApiError` accessors ([Execution
policy](execution_policy.md#errors)). They report what happened as of the
recorded stage, never a remote rollback or business outcome inferred from
transport.
