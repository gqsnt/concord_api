# Concord v3 mental model

## One contract, several views

A Concord declaration is one closed `api!` invocation, resolved once into a
semantic model and projected into task-specific views:

```text
source tree
  = generated navigation tree   (client -> scope handles -> named requests)
  = unbound navigation tree     (CatalogApi::unbound() -> intents)
  = static contract tree        (CatalogApi::contract(), description())
  = generated rustdoc           (method and request-type documentation)
```

The views differ in authority, not meaning. No separately edited
documentation model exists that could disagree with the lowering.

## Owners and bare names

Every client, scope and endpoint is an **owner** with its own values and
attachments. A bare name means the current owner only; an ancestor value is
qualified by its owner (`client.tenant`, `regional.region`). A missing bare name
is an error even when an ancestor has a matching name, so a later parent edit
cannot silently capture a local reference.

## Static contract state

- the client/scope/endpoint tree and exact Rust value declarations;
- presence (`?`), initializers (`= V`, `#[default]`), sensitivity and owners;
- request/response representation defaults, scope fences and inline `via`;
- the ordered path/DNS templates and the symbolic query/header program;
- credential sources and ordered auth placements;
- rate policy IDs, ordered key labels, windows and costs;
- status, expected absence, effect, pagination Init/Cursor mappings and
  capability terminals;
- explicit and navigation-derived endpoint IDs and provenance.

## Built-client state

- client values, with defaults evaluated once when the builder was created;
- the fixed logical origin;
- transport and runtime configuration;
- the retry policy and its shared load budget;
- the explicitly selected limiter;
- credential slots, each with an installation-intent epoch and an installed
  version;
- the read-only observer and debug policy.

Built client values are immutable: changing root configuration means building
another client. Clones share services, credential stores and snapshots.
Independently built clients share nothing unless the application passes them
the same service object (for example one `Arc<LocalRateLimiter>`).

## Inert selected state

A scope handle, endpoint intent or request holds immutable selected values and
local controls. Scope defaults are evaluated once when the scope is selected;
endpoint defaults once when the request or intent is constructed. Changing a
scope control consumes the handle and returns a new snapshot; requests created
earlier keep the old one. Construction, cloning, description and planning never
perform HTTP, provider, limiter, body preparation, retry or observation work.

Binding an unbound intent (`client.request(intent)`) supplies the client's root
values and services without evaluating retained defaults again; direct and
unbound+bind requests have equivalent plans for the same selected snapshots.

## Terminals and operations

A request is a value until you poll a terminal:

| Terminal | Resolves to |
|---|---|
| `.await` | the declared output (one response; for a streaming output, an opened stream) |
| `.response()` | `ApiResponse<Output>` with status, version, headers, URL and metadata |
| `.accepted_bytes()` (declared) | `ApiResponse<Bytes>`, bypassing representation conversion |
| `.store_as_x()` (credential source endpoint) | `StoreOutcome` |
| `.collect_prefix(n)` / `.collect_all(n)` / `.pages(n)` (paginated) | `Prefix<Item>` / `Vec<Item>` / a lazy `Pages<Item>` |

Polling creates one **operation** with an identity and one absolute deadline.
A direct operation may contain several visible attempts. A traversal contains
page operations, each of which may contain visible attempts, all under the one
aggregate deadline.

```text
operation (deadline starts at first poll)
  |- visible attempt
  |    `- at most one transport-private physical recovery (proven unprocessed)
  |- policy retry           (RetryPolicy, gated by effect/body/budget/deadline)
  |- auth-recovery replay   (at most one, provider refresh only)
  `- fixed outcome          (then post-fixation observation)
```

## Effect class

- `safe`: read-like requested semantics; eligible for pagination and for retry
  when every other gate allows;
- `idempotent`: repeatable with the same intended effect; eligible for retry
  after possible dispatch, not for pagination;
- `non_idempotent`: after possible or observed dispatch, never resubmitted
  automatically.

Method defaults are `safe` for `GET`/`HEAD`/`OPTIONS`, `idempotent` for
`PUT`/`DELETE` and `non_idempotent` for `POST`/`PATCH`/custom methods. An
explicit `effect` is the author's provider-contract assertion. Body
replayability is a separate mechanical fact.

## Public and private information

```text
ordinary public protocol data      sent; shown in plans
sensitive public protocol data     sent; withheld from automatic observation
body and response data             application data; never in automatic output
private credential data            applied after public shaping; never observed
operator diagnostics               bounded, sanitized, value-free
```

Sensitive public data still reaches the server. Sensitivity of a query/header
key is sticky along the root-to-owner chain, so a later public override cannot
declassify it. Credential material never enters public shaping.

## Representations and delivery

Request and response representations describe wire/media conversion. The exact
endpoint output type describes what the caller receives and when:

```concord-pseudocode
GET item() -> Item via Json;                    // one decoded value
GET rows() -> Vec<Row> via Csv;                 // complete bounded collection
GET watch() -> ItemStream<Event> via NdJson;    // lazy record stream
GET download() -> ByteStream via Binary;        // lazy bytes
GET find(id: Id) -> Option<Item> via Json {     // head-only expected absence
    absent on 404;
}
```

## Pagination

Pagination is an explicit, safe aggregate. A controller is started from a
public `Init` value, maps its whole `Cursor` onto one endpoint slot, receives
one complete accepted page and returns items plus either `Done` or the next
state. It never receives a native response or arbitrary request mutation; core
validates the next cursor, progress identity and bounds before delivering the
current page.

## Four projections, one source of meaning

| View | Answers | Must not imply |
|---|---|---|
| Method/hover rustdoc | What to call, required inputs, result, capabilities, inherited auth/timeout | That runtime values or provider behavior are known |
| Static contract (`contract()`, `description()`) | Effective symbolic behavior, origins, fallbacks, coverage, identities | That a plan was executed |
| Bound `RequestPlan` | This request's public shape and effective controls | Credential acquisition, quota admission, body preparation or a send |
| Execution/stream/traversal outcome | What happened as of the recorded stage and why it stopped | Remote rollback, complete consumption at acquisition, business intent |

Do not use one concept for all four.
