# Concord Next — families E and F — codex

These are design proposals, not implemented syntax. Repository evidence is from
`ba10189b5e4e4777f622062a07175b7ec1d9c366`. All `examples/...:line` references below
are relative to `/home/canarit/projects/Concord_Next/work/wt/rv-p1`; ranges are
inclusive. Before snippets are excerpts of that tree; after snippets show the
proposed language or Rust API. Surrounding declarations remain as they are unless
stated otherwise. Estimated deletions are physical source lines, not the
classifier's non-blank-line measure, and are not additive between designs.

I read the charter, E/F seeds, construction problem, glue inventory, current
language guides, relevant macro/core implementation, and the operation and paging
paths of the supplied witnesses and controls. The aggregate
`work/inventory/FRICTION.md` does not exist at this checkout, so the evidence uses
the SDK-local `FRICTION.md` and available `MAINTENANCE.md` instead. Historical notes
sometimes name wrappers that have since disappeared; deletion claims below use
current source, not those historical line numbers. No repository file was edited.

## Evidence and design boundary

The inventory reports 44 non-blank `glue:operation` lines and 4,006
`glue:paging` lines across the gallery. Its operation category is not a reliable
upper bound: current `Class::of` records in Strava, Sentry, PayPal and Zendesk
contain provider facts inside code otherwise classified as behavior. Conversely,
the large GitHub paging excerpt includes repository-rename and restoration
semantics; it cannot all be credited to a generic cursor feature.

| Witness | Current evidence that constrains the proposals |
| --- | --- |
| Strava | `src/auth.rs:273-307` combines read/write, quotas, OAuth scope and an SDK retry delay. `MAINTENANCE.md` records an initially compiling update whose quota and uncertainty handling were wrong. `src/lib.rs:216-265` advances page numbers with overflow checks. |
| Sentry | `src/policy.rs:88-117` repeats write and paged facts; `src/pagination.rs:45-136` implements its particular Link grammar, results flag, cursor agreement and trust checks. |
| Kraken | `src/lib.rs:50-66` declares POST reads; `src/policy.rs:78-111` signs final form bytes with a fresh nonce. `src/trades.rs:44-94` is an offset/count traversal, not an opaque-cursor traversal. |
| PayPal | `src/policy.rs:129-158` repeats which endpoints accept request IDs. Maintenance explicitly records that omitting one endpoint from that list compiled and broke its timeout test. Retention is endpoint-specific. |
| Zendesk | `src/policy.rs:161-189` repeats effect and the special meaning of an update's 409; `src/pagination.rs:39-61` checks `has_more`, cursor progress and a corroborating link. |
| Datadog | `src/api.rs:58-68` contains both a POST search read and a POST event submission; `src/client.rs:154-198` repeats body-cursor construction, cycle guards and projection. |
| Square | `src/error.rs:25-54` distinguishes uncertain writes and keyed creates; `src/lib.rs:135-159` is a small cursor adapter. **This checkout has no Square search endpoint**: the supplied POST-search counterexample must be a proposed extension fixture, not a fabricated before snippet. |
| GitHub | `src/paging.rs:116-142` classifies listings; `src/pages.rs:47-198` builds factories per endpoint. `src/paging.rs:454-579` handles restored links, alternate repository paths and parents. |
| Notion | `src/api.rs:22-31` puts one cursor in JSON and another in query. `src/client.rs:105-115` refuses incomplete query results; `:137-202` repeats traversal scaffolding. |
| Discord | `src/api.rs:149-178` checks strict descending IDs, page size and the requested bound. Its friction note calls short-page termination an SDK inference. It is not a universal paging fact. |
| Stripe | `src/pages.rs:126-193` bridges generated operations to a handwritten trait; `:333-351,456-494` derive directional cursors and reject empty nonterminal/stalled pages. `:501-520` fingerprints selected request context. |
| Mailgun | `src/events.rs:263-271` rejects mismatched resume context synchronously. `:285-310` follows links through its SDK policy and terminates on an empty page. `tests/first_use.rs:952-955` pins eager refusal. |
| DigitalOcean | `src/pagination.rs:28-109` has four start/resume pairs; `:169-195` binds collection and tag filters. Maintenance records both the twins and the current whole-page refusal on an unsafe next link. |
| Intercom | `src/pages.rs:24-42` differs only in query/body cursor placement; `:59-75` projects nested data/cursor/count/head. |
| Microsoft Graph | `src/paging.rs:138-178` distinguishes next links from final delta checkpoints and validates objects before emitting them. An empty page can continue. |
| Adyen | `src/policy.rs:62-80` confirms a capture against the sent body/path; `:83-89,306-329` captures replay identity from the actual prepared request. An enum variant is insufficient evidence. |
| Kubernetes | `src/list.rs:299-328` checks snapshot equality before accepting a chunk; `:277-279` clears first-request version selectors on continued requests. |
| Elasticsearch | `src/pit.rs:443-453` stores the newest PIT ID **before** rejecting partial results. Sort values retain their original JSON bytes. These operations are not interchangeable with a scalar cursor. |

The simple controls remain byte-for-byte unchanged at the declaration level:
Nager's `src/lib.rs:15-39`, PokéAPI's existing two endpoints, and Frankfurter's
existing two endpoints acquire no mandatory annotations, enum statement, paging
dependency or new constructor argument in any proposal.

Three distinctions apply to every design:

1. **Read is a provider effect, not an HTTP-method test or a retry instruction.**
   Byte reproduction, nonce allocation, attempt bounds and status decisions stay
   in Rust.
2. **Uncertainty is an outcome, not an endpoint Boolean.** The policy combines
   declared effect/key support with `Delivery`, the observed status, body failures
   and provider semantics. A write can be uncertain despite receiving a head.
3. **A page's items, its completeness and its continuation are different evidence.**
   No design silently turns a refused continuation into exhaustion, swallows a
   failed page, or treats an empty page as terminal without a provider contract.

# Family E — operation facts

## E1. Put effect on the existing operation descriptor — smaller than you think

**Fact it expresses:** An operation observes provider state or can change it,
independently of its HTTP method.

**Declaration (author writes):** Datadog, `examples/datadog-events-sdk/src/api.rs:60-68`.

Before:

```rust
POST search(body request: SearchRequest) -> EventsPage { path "v2/events/search"; }

POST post(body event: NewEvent) -> EventAcceptance {
    path "v1/events";
    request PostJson;
}
```

After:

```rust
POST search(body request: SearchRequest) -> EventsPage {
    path "v2/events/search";
    effect read;
}

POST post(body event: NewEvent) -> EventAcceptance {
    path "v1/events";
    request PostJson;
    effect write;
}
```

The existing GET `list` at `src/api.rs:45-56` also receives `effect read;` in its
body. These facts attach to `concord::Operation`, including operations obtained
without preparing a request. There is no new operation registry or mandatory
enum. Existing `operations DatadogApiOperation;` and its dispatch continue to work.
An absent fact is `None`, never inferred as a read from GET, a method name or
`Clone`. Contradictory or duplicate effects are declaration errors.

Implementation trap: `crates/concord-core/src/response.rs:35-44` currently has
both an EndpointMeta-backed identity and a separate declared name/method form.
Adding a field only to EndpointMeta would make `E::operation()` lose the fact.
Both generated paths must reference the same immutable contract data, and errors
must carry it through native decoder execution. Keep the existing public
name/method equality semantics distinct from this metadata; equality is not a
credential or replay-authority check.

**Call site (caller writes):** Public caller unchanged:

```rust
// Before and after
let page = client.events().search(search).send().await?;
```

The policy caller changes from a second endpoint table to a typed fact:

```rust
// Before, src/api.rs:107
let posting = posting(operation);

// After; MissingEffect is mapped to a local SDK error before dispatch.
let effect = operation.operation().effect().ok_or(Error::MissingEffect)?;
let posting = effect == concord::Effect::Write;
```

The name `posting` describes this bounded slice: its only write is the legacy
post. Its 202-specific success rule remains Rust and must remain scoped to that
operation if another kind of write is introduced. `effect write` does not mean
"202", "uncertain on every error", or "never retry".

**SDK code that disappears:**

- `examples/datadog-events-sdk/src/api.rs:88-97`: the documented `posting` match,
  10 physical lines; its call at `:107` becomes the fact lookup.
- `examples/discord-rest-sdk/src/limiter.rs:85-88`: the four-line method-based
  `is_write` implementation can become a descriptor lookup after its endpoints
  state their effects. Keep `failed_before_head` and its error construction.
- In Sentry, the **write column** of
  `examples/sentry-sdk/src/policy.rs:90-115` disappears; the paged column remains
  until a paging design supplies that fact. Do not count the entire 30-line Class
  as removed by E1 alone.

About 10 primary-witness lines become two endpoint facts plus the GET fact;
the larger benefit is eliminating the second list of endpoint identities. It
does not erase all of Strava's Class: retry delays are SDK behavior, and quotas
and required scopes need other facts. This deliberately small mechanism also
feeds uncertainty reporting in family A and removes operation forwarders in G.

**Hardest counterexample:** **Kraken:** `account.balances`, `open_orders` and
`trades_history` are explicitly reads despite POST. The signing policy still
allocates a nonce for each attempt and signs the final encoded payload; reading
`Effect::Read` never grants permission to clone and replay `Signed<T>`.
**Datadog/Square search:** add a POST read fixture; the existing Datadog search is
the real witness. **Adyen:** this fact provides no request-equality proof.

**Simple control (Nager):** Unchanged. Its existing `SendWith` policy need not
read effects; an unannotated descriptor returns `None`.

**Charter check:**

- C1: `read`/`write` state provider effects; no `retryable` or `uncertain` switch.
- C2: policies read effects; delivery interpretation and retry decisions stay Rust.
- C3: no required annotation or changed send trait for any simple control.
- C4: absence stays unknown; HTTP method never supplies an invisible default.
- C5: duplicate/conflicting facts point to the authored `effect` statement.
- C6: caller-facing explanation is “This operation reads/changes provider state.”
- C7: `prepare`, `into_parts`, `send_raw` and native edits remain available.
- C8: querying metadata performs no I/O, allocation, retry or registration.
- C9: require POST-read wire tests, unknown-fact tests, cfg tests and compile-fail
  duplicate-effect tests; verify the same fact on endpoint, Prepared and errors.

**New concepts:** `effect read;`, `effect write;`, `Effect`,
`Operation::effect() -> Option<Effect>`.

**Breaking changes:** Core addition can be additive. Adopting the strict lookup
in Datadog adds a local missing-fact error case; downstream exhaustive matches on
that SDK error need migration. Existing enum and `SendOperation` APIs remain.

**Why it might be wrong:** It removes a small table, not the provider's entire
contract; calling it a complete retry solution would misrepresent its scope.
An effect alone cannot express keyed replay, quotas, permissions or partial success.

## E2. Typed provider fact records, inherited through the real scope tree

**Fact it expresses:** A Zendesk operation's effect and the provider meaning of
its 409 response belong to that operation, with shared facts inherited once.

**Declaration (author writes):** Zendesk,
`examples/zendesk-support-sdk/src/lib.rs:94-103`.

Before:

```rust
scope ticket(id: u64) {
    path "tickets/{id}";
    limit TICKET_BODY_LIMIT;

    GET show() -> TicketResponse;

    PUT update(body ticket: TicketUpdate) -> TicketResponse;
}
```

After:

```rust
scope ticket(id: u64) {
    path "tickets/{id}";
    limit TICKET_BODY_LIMIT;
    facts ZendeskContract {
        effect: Effect::Read,
        conflict: ConflictMeaning::Unspecified,
    }

    GET show() -> TicketResponse;

    PUT update(body ticket: TicketUpdate) -> TicketResponse {
        facts ZendeskContract {
            override effect: Effect::Write,
            override conflict: ConflictMeaning::StaleTicket,
        }
    }
}
```

The record type is ordinary SDK Rust:

```rust
#[derive(Clone, Copy)]
pub struct ZendeskContract {
    pub effect: Effect,
    pub conflict: ConflictMeaning,
}
```

The collection's list/create endpoints receive records at their own real scope.
These are **not** arbitrary policy profiles: fields must describe provider
contract data. A `max_attempts`, backoff delay or `check_scope_locally` field
would violate C1 even if it type-checked. In particular, copying Strava's entire
current Class into one of these records would be wrong.

The compiler merges explicitly named fields along the scope chain and emits a
complete Rust struct literal per leaf. There is no computed default or struct
update expression hiding an omitted field. The same inherited-field conflict
rule requires `override` for a replacement. A declared record used by several
leaves must be complete at every such leaf; its Rust type need not be resolved
when all those leaves are cfg-disabled.

**Call site (caller writes):**

```rust
// Public caller, before and after
let updated = api.ticket(id).update(change).send().await?;

// Policy before, src/policy.rs:199-202
let Class { read, collision_on_conflict } = Class::of(operation);

// Policy after
let facts = operation.operation()
    .fact::<ZendeskContract>().ok_or(Error::MissingContract)?;
let read = facts.effect == Effect::Read;
let collision_on_conflict = facts.conflict == ConflictMeaning::StaleTicket;
```

`Operation::fact<F>()` returns an immutable typed reference from the generated
descriptor. The descriptor contains local static data; there is no global TypeId
registry or policy lookup by endpoint-name string. Runtime decisions use the
record through ordinary Rust. Absence is explicit and may be refused before I/O.

**SDK code that disappears:**

- `examples/zendesk-support-sdk/src/policy.rs:161-189`: Class and its endpoint
  match, 29 lines; `:199-202` becomes a typed lookup. Existing read/write execution
  at `:203-209`, conflict error construction and Retry-After decisions remain.
- Kraken's public/private signing classification at
  `examples/kraken-spot-rest-sdk/src/policy.rs:83-90` becomes a fact test if its
  account scope declares a typed signing-scheme fact. Its signing body at
  `:91-110` is behavior and stays.
- Strava can move provider quota membership and required permission into facts
  while retaining the delay selection at `src/auth.rs:289-291` in Rust.
  The precheck is also a Rust choice: declaring `activity:write` for uploads must
  not silently introduce a new local scope refusal that the old SDK omitted.

Primary deletion is 29 lines, offset by the schema type and declarations; this
is principally removal of independently maintained endpoint tables, not a claim
of immediate net line savings in a four-operation SDK. It applies across E,
auth facts, rate-limit facts and A's status meanings, with one typed lookup
mechanism instead of a keyword for every provider.

**Hardest counterexample:** **Datadog POST search:** effect is an explicit record
field, never derived from method. **Policies that ignore operations:** keep
using `SendWith`; they need not name or request a record. **Cfg endpoints:**
records and lookups carry exactly the endpoint's inherited cfgs, like today's
operation enum. **Adyen:** a static record can describe key scope/retention but
cannot certify the current request; E3's explicit request inspection is still needed.

**Simple control (Nager):** Unchanged; no record type, `facts` block, enum or lookup.

**Charter check:**

- C1: record fields are provider facts; SDK tuning fields are explicitly excluded.
- C2: declarations own contract values; Rust owns their interpretation and actions.
- C3: fact records are absent from every simple control.
- C4: inheritance is field-wise, visible and uses explicit `override`.
- C5: missing/unknown/wrongly typed fields are reported at their authored names;
  span the emitted Rust literals at those names, not internal descriptors.
- C6: generated docs show a short table of effective facts, not merge algorithms.
- C7: callers can inspect facts and still execute the native reqwest request.
- C8: only immutable static records; no registration, callback execution or scheduler.
- C9: compile-fail incomplete records and invalid overrides; test cfg-disabled
  types, inherited facts and identical endpoint/received/error lookups.

**New concepts:** `facts Type { field: value }`, field `override`, immutable fact
records, `Operation::fact<F>()`. Values are checked const data, not closures.

**Breaking changes:** Additive core API. SDKs may keep old enums while deleting
private Class types. A missing-contract error variant or newly exported schema
type is an SDK migration; no dispatch trait change is inherently required.

**Why it might be wrong:** This is an attractive way to smuggle a configuration
language into Concord; type safety does not prove a field is a provider fact.
For small APIs, the schema and inheritance rules can cost more than the deleted table.

## E3. Name the idempotency key relation; inspect it on the request being sent

**Fact it expresses:** PayPal treats a particular request value as the operation's
idempotency key; the key's presence alone is not proof that a replay is permitted.

**Declaration (author writes):** PayPal,
`examples/paypal-sdk/src/lib.rs:108-118`.

Before:

```rust
POST authorize(
    body request: AuthorizeRequest,
    request_id?: RequestId,
) -> AuthorizedOrder {
    path "authorize";
    headers {
        "paypal-request-id" = "{request_id}";
        "prefer" = "return=representation";
    }
}
```

After:

```rust
POST authorize(
    body request: AuthorizeRequest,
    request_id?: RequestId,
) -> AuthorizedOrder {
    path "authorize";
    headers {
        "paypal-request-id" = "{request_id}";
        "prefer" = "return=representation";
    }
    idempotency request_id;
}
```

`request_id` already has a wire placement; the fact names that value instead of
repeating its header string. Place the fact on every supported endpoint,
including the no-body `authorization.void`. Do not infer it merely because a
header happens to have an idempotency-like name. Do not assign Orders' six-hour
retention to the Payments operations: the current maintenance record expressly
declines that inference.

The descriptor exposes where this key is placed. For a header value, the generic
reader reports absent, one value, or invalid/repeated values. Provider key syntax
is still validated by `RequestId::new`. An optional small Rust
`RequestSnapshot::capture(&request, byte_bound)` stores the method, full URL,
all authored headers and fixed encoded body for exact equality. It is redacted,
does not use a collision-prone hash as proof, and creates no ledger or retry loop.

**Call site (caller writes):**

```rust
// Public caller, before and after: omission is still allowed.
let order = api.order(id).authorize(request).set_request_id(key).send().await?;

// Policy before, src/policy.rs:211-217
let request_id = prepared.request().headers()
    .get("paypal-request-id")
    .and_then(|v| v.to_str().ok())
    .and_then(|v| RequestId::new(v).ok());
let permitted = !write || (request_id.is_some() && accepts_request_id);

// Policy after, schematic names for the proposed typed descriptor API
let request_id = E::operation().idempotency()
    .map(|fact| fact.read_header(prepared.request()))
    .transpose()?                       // retains duplicate/invalid distinction
    .flatten()
    .and_then(|v| v.to_str().ok())
    .and_then(|v| RequestId::new(v).ok());
let permitted = !write || request_id.is_some();
```

The last line preserves this PayPal policy's existing decision. It is not a core
rule. The read/write distinction can remain its current match or come from E1.
In a policy that edits the native request, read the fact and capture identity
**after** those edits, on the owned request handed to `Decoder::execute`.

**SDK code that disappears:**

- The `accepts_request_id` column in
  `examples/paypal-sdk/src/policy.rs:133-135,146,154,205` and its use at `:217`:
  about 7 lines/sites. The write column and its match remain if E1 is not adopted.
  Do **not** credit the entire `:129-158` Class to E3 alone.
- The hardcoded header selection at `:211-216` is replaced by a descriptor reader;
  key parsing stays. This is removal of a duplicated fact, not a large net LOC win.
- With the optional exact request snapshot helper,
  `examples/adyen-checkout-sdk/src/policy.rs:83-89,324-329` disappear: 13 lines of
  SDK-local Identity representation/construction, replaced by one explicit Rust
  capture. `Entry`, `reserve`, expiration, revocation and confirmation stay.

The relation also benefits family D's key redaction and F's continuation binding:
the same descriptor can identify a cursor/key slot without an independent
string allowlist. The snapshot utility is shared Rust request machinery, not a
provider guarantee or an implicit enforcement layer.

**Hardest counterexample:** **Adyen:** do not fingerprint an endpoint enum or its
pre-serialization input. Capture the fully decorated request at dispatch; compare
the changed URL, headers and encoded bytes. Keep `Confirm::confirms` at
`src/policy.rs:62-80`, including response echo checks, and all company ledger
rules. The snapshot certifies the SDK's reqwest request at that boundary, not
unobservable proxy transformations or redirects. The strict transport requirements
remain. A streamed/non-buffered body returns “not representable as fixed identity”;
read status never manufactures replayable bytes. **Square:** a JSON-body key needs
an explicitly typed body-field binding; the header-only first version of this
design does not claim to remove Square's key extraction or model checks.

**Simple control (Nager):** Unchanged; no key relation or snapshot capture.

**Charter check:**

- C1: the declaration identifies the provider's key, not a retry permission switch.
- C2: key support is metadata; window enforcement, replay and persistence stay Rust.
- C3: no changes for clients without provider keys.
- C4: resolve a declared value and its placement; never guess from a header name.
- C5: an unknown/unplaced value, or an ambiguous placement, fails at `idempotency`.
- C6: docs say “Idempotency key: request_id”; retry policy has its own short docs.
- C7: native edits are allowed; readers inspect the edited request explicitly.
- C8: reading/capturing does not reserve a key, retry, persist, refresh or dispatch.
- C9: mutate key, method, target, header and body independently; check missing and
  repeated keys, no-body voids, one-shot bodies and redacted snapshot diagnostics.

**New concepts:** `idempotency value;`, `IdempotencyFact`, strict key reader;
optional Rust `RequestSnapshot` with bounded fixed-body capture.

**Breaking changes:** Core can be additive; strict duplicate-key reading changes
previously permissive native-edit cases and needs an explicit SDK migration.
Replacing Adyen's private Identity is internal if its equality and error mapping
remain exactly the same. No generated key, ledger or transport policy is added.

**Why it might be wrong:** Static key support is only a small part of a replay
contract; retention, key scope and final-request confirmation can dominate the code.
A snapshot taken before later request edits would provide misleading assurance.

# Family F — paging facts

## F1. Two typed page projections, with the existing Rust paging engine — smaller than you think

**Fact it expresses:** Square's returned `payments` are the items, and its returned
cursor is placed in this operation's query cursor; an empty cursor means completion.

**Declaration (author writes):** Square,
`examples/square-payments-sdk/src/lib.rs:78`.

Before:

```rust
GET list(#[sensitive] query cursor?: String, query limit?: u32) -> PaymentsPage;
```

After:

```rust
GET list(#[sensitive] query cursor?: String, query limit?: u32) -> PaymentsPage {
    page items response.payments;
    page cursor request.cursor <- response.cursor, empty = end;
}
```

This generates only typed projections: a cursor slot on the request and an
items/next projection on its declared output. They are ordinary field accesses,
not JSONPath or deserialize-to-Value/re-encode operations. Request and response
cursor types must agree. The optional `empty = end` describes Square's sentinel;
it is not a universal default. An empty items vector does not end this listing.

The compiler supplies `CursorSlot<K>` and a typed `PageProjection` on the endpoint
type, not an overlapping implementation on a shared output model; the optional
`concord-pagination` crate consumes them through a Rust function. Core gains no
pagination loop or mandatory capability dependency. `Resume::Start` replaces the
template's cursor with absence; `Resume::At(k)` replaces it with k, as today's
Square adapter does. Generated projections retain the rest of the envelope as
typed page metadata, with the items moved out. They do not silently discard
diagnostics, counts or links. For `Response<Envelope>` wrappers, a single ordinary
Rust wrapper adapter separates the document and metadata; it is not repeated per
endpoint and is not guessed by matching a type's spelling.

**Call site (caller writes):**

```rust
// Before
let mut pages = api.payments().list().pages(from, cap);

// After: explicitly choose the Rust traversal capability.
let mut pages = concord_pagination::cursor_pages(api.payments().list(), from)
    .max_pages(cap);
```

The function sends each fresh endpoint through its generated `send()`/`Sends`
implementation, preserving the SDK error and response policy. A direct
`list().send()` still returns one `PaymentsPage`. A cap produces the existing
bounded outcome; a later send failure preserves earlier accepted pages and the
failed page's key. There is no generated eager network operation.

**SDK code that disappears:**

- `examples/square-payments-sdk/src/lib.rs:135-159`: the entire 25-line handwritten
  inherent paging adapter. Two facts replace its per-endpoint projection and
  cursor-setter selection; the generic Rust capability replaces its factory.
- Intercom can remove its two placement-specific adapters at
  `examples/intercom-sdk/src/pages.rs:19-43` (25 lines) after declaring the query
  cursor and JSON-body `search.pagination.starting_after` relation. Its existing
  private-model accessor remains until available as a checked Rust field/trait
  projection; this design never edits private fields through serialized JSON.
  Keep its cursor validity and PageMeta interpretation at `:59-75`.
- Notion's plain children projection at
  `examples/notion-sdk/src/client.rs:196-202` becomes generated, 7 lines. Its query
  completeness check `:105-115` and envelope checks `:204-222` remain Rust.

The 25 primary-witness lines are mostly glue, so this is the minimum version
that removes most of a simple cursor adapter. Do not claim it removes GitHub's
577 classified paging lines or Kubernetes' consistency implementation. It also
removes family B's per-endpoint envelope projection, E's paged classification
(`Operation` can expose the presence of a page contract), and G/H's adapter
methods and repetitive resume prose.

**Hardest counterexample:** **Discord:** deliberately not expressible as a plain
scalar field cursor. Keep `history_page` and its strict ordering/size/bound checks;
never replace them with `last().id`, `min(id)`, sorting or deduplication. A Rust
adapter can use the generated request slot while retaining that validator.
**Notion/Elasticsearch:** validate query completeness/shards before projecting
items. **Mailgun:** use F2 or keep its current adapter; scalar projection does
not cover link trust or eager resume validation. **Kraken trades:** keep its
offset/count arithmetic and fresh signing; no “all pages are cursors” inference.

**Simple control (Nager):** Unchanged. In particular, returning `Vec<Holiday>` does
not imply pages. PokéAPI's existing manual link API also stays unchanged.

**Charter check:**

- C1: item and cursor locations and an empty sentinel are provider facts.
- C2: typed projections expose facts; caps, traversal, cycle policy and retry stay Rust.
- C3: only annotated endpoints gain projection traits; no control declaration grows.
- C4: explicit request/output fields; no name or GET/POST inference.
- C5: bad fields, unsupported outputs and cursor-type mismatches point to the selector.
- C6: docs name items, next cursor and its request slot in three short lines.
- C7: single-page send and all reqwest escapes stay intact; formats are not privileged.
- C8: construction/projection adds no I/O or hidden traversal.
- C9: wire-test cursor replacement and unchanged filters, empty intermediate pages,
  empty sentinel, cap/failure checkpoints and retained metadata; compile-fail bad selectors.

**New concepts:** `page items`, `page cursor`, explicit `empty = end`,
`CursorSlot`, `PageProjection`, `cursor_pages` in the capability crate.

**Breaking changes:** Additive traits/function. Deleting the SDK's inherent
`.pages` method changes callers to the shown capability call; changing page meta
from `()` to a typed envelope remainder changes explicit traversal type annotations.
Keeping a deprecated delegator temporarily is a migration option, not counted as
a completed deletion. Slot traits are additive rather than new required methods
on `Endpoint` or `Sends`.

**Why it might be wrong:** Even two projections leave provider validity checks in
Rust; promising a universal `.pages()` would conceal the difficult cases.
Typed access across private body models may require a small model accessor instead of total glue removal.

## F2. A server link is a bound request continuation, preserving SDK dispatch

**Fact it expresses:** Mailgun's `paging.next` names the next request of this same
event search, rather than a cursor to splice into a reconstructed query.

**Declaration (author writes):** Mailgun,
`examples/mailgun-sdk/src/lib.rs:83-93`. The existing parameter list is unchanged.

Before:

```rust
GET events(
    query(..) range: EventRange,
    query limit: PageSize = PageSize::default(),
    #[sensitive] query("message-id") message_id?: MessageId,
    #[sensitive] query recipient?: String,
    query event?: String,
    #[sensitive] query tags?: String,
) -> EventPage {
    path "events";
}
```

After:

```rust
GET events(
    query(..) range: EventRange,
    query limit: PageSize = PageSize::default(),
    #[sensitive] query("message-id") message_id?: MessageId,
    #[sensitive] query recipient?: String,
    query event?: String,
    #[sensitive] query tags?: String,
) -> EventPage {
    path "events";
    continuation next <- response.paging.next;
}
```

This is a typed response relation and a new request representation:
`Continuation<E, Context>` holds the original endpoint/dispatch, a validated
server target and a context binding. It is not a universal URL setter. A Rust
`FollowPlan` supplies the SDK's pure link validation, context equality, item
projection and terminal decision. Mailgun retains `trusted_link` and its
empty-page rule in Rust. Binding, redaction, cycle bookkeeping and retained
endpoint/decoder/policy are shared machinery.

The binding context is explicit: for Mailgun it is the original prepared URL
including query plus the authorization value, exactly as today. It is not
`Debug` output or only the origin. For Graph it is the stable tenant/principal
context, collection and selection rather than a rotating bearer-token string.
The generic plan compares the supplied typed context; it does not infer identity
from secret values or serialize credentials into checkpoints.

**Call site (caller writes):**

```rust
// Before
let mut pages = events.pages_from(from, cap)?;

// After: MailgunEvents is the remaining ordinary Rust provider adapter.
let plan = concord_pagination::FollowPlan::new(events, MailgunEvents)?;
let mut pages = plan.pages(from, cap)?;
```

`plan.pages` validates the **starting** continuation synchronously before
returning its Result, preserving Mailgun's eager failure. Network reads remain
lazy. Following a link executes the same generated dispatch as the first page:
the policy sees a `Followed<E>` implementing `Endpoint<T>` with E's identity.
Generated code supplies a small typed dispatch handle that captures the selected
policy value and, when relevant, the operation variant. It invokes that policy
with the followed endpoint. Merely sending a `Prepared` directly is insufficient.
`Sends<O>` keeps its existing generic output contract; no extra trait bounds are
added inside a handwritten `SendWith::send` implementation.

The URL's query bytes/order are followed as returned, subject to the existing
SDK trust rule; they are not rebuilt from the template. A policy that signs
requests still runs on the final target. The existing explicit
`Prepared::follow` escape remains a separate native path.

**SDK code that disappears:**

- `examples/mailgun-sdk/src/events.rs:239-245`: seven-line EventCursor storage
  becomes the shared redacted bound continuation.
- `:263-271`: nine lines of eager original/credential capture and context check
  move into the explicit plan/context adapter.
- `:274-280` and `:285-291`: fourteen lines of cloning/target selection and split
  initial-versus-followed dispatch become the generic follow driver.
- `:296-301`: six lines of bound-cursor construction become generic. About 36
  gross lines above disappear; allow roughly 10–15 new Rust adapter lines to
  specify context/projection/terminal semantics. Keep `trusted_link` at `:314-338`
  and keep empty-page termination at `:292-295`.
- DigitalOcean can remove `NextLink`'s private storage/Debug at
  `examples/digitalocean-sdk/src/pagination.rs:9-17` and replace its generic driver
  `:135-167` with a shared FollowPlan. Its `validate`/filter equality at `:169-195`
  remains provider trust code. This is another roughly 42 gross lines, not all
  167 lines assigned to paging by the classifier.

This mechanism removes glue across F, G and B: one request continuation keeps
the existing policy, error/result shape and endpoint identity. It also avoids
duplicating the generated send path described in Mailgun FRICTION F3.

**Hardest counterexample:** **Mailgun:** synchronous context mismatch remains
observable at `.pages(...)`, before polling or I/O. **GitHub:** equal-path trust
is too strict: accept the SDK's existing repository-ID/rename/parent validator;
its restored-link/path logic at `src/paging.rs:454-579` stays. A header relation
can be described by `continuation next <- header Link(rel = "next");` using the
existing Link parser, but Sentry's `results` flag/cursor agreement remain its
adapter. **Graph:** delta checkpoints stay a distinct type, not `next`; preserve
`src/paging.rs:138-178` cardinality/object checks.

If a later continuation fails, the paging engine retains **previously accepted**
records and the failed position. Mailgun/DigitalOcean currently reject the
current page before accepting its records when its next link is invalid; this
design preserves that choice. It does not claim to solve “accept current items
but defer a continuation error.” That requires an explicit separate page/next
outcome type; swallowing the error or refetching after an application commit is
not an acceptable substitute.

**Simple control (Nager):** Unchanged. PokéAPI can retain its current `.follow`
without adopting a FollowPlan or adding a declaration line.

**Charter check:**

- C1: `continuation` states the response's next-request relation, not a trust/retry toggle.
- C2: relation in the declaration; trust, context identity and terminal decisions in Rust.
- C3: no mandatory FollowPlan, new constructor or generated paging method on controls.
- C4: explicit relation and explicit Rust validator; preserve the server query.
- C5: malformed selectors fail at the declaration; resume-context errors have public types.
- C6: “Follow this next link with the same SDK policy” explains the caller path.
- C7: native prepare/follow/receive/into_parts remain independently available.
- C8: no network work at construction; validation timing is explicit, not background work.
- C9: test eager Mailgun mismatch, same-policy error types, credential/collection changes,
  query-byte preservation, GitHub alias paths, cycles and final Graph delta metadata.

**New concepts:** `continuation next`, `Continuation<E,C>`, `Followed<E>`,
generated typed dispatch handle, Rust `FollowPlan` and provider adapter.

**Breaking changes:** New core/capability APIs can be additive. Removing SDK-local
EventCursor/NextLink types changes named public cursor types and stored cursor
representations. Storage needs versioning; no cross-version equality or
authenticated-storage guarantee is implied. Preserve each SDK's existing error
mapping and eager/lazy timing during migration.

**Why it might be wrong:** A reusable context/dispatch abstraction may be larger
than the deleted driver, particularly for SDKs with only one linked endpoint.
Generic equality cannot replace provider path trust, principal identity or a secure checkpoint store.

## F3. Composite continuation contracts for snapshot-bound chunks

**Fact it expresses:** A Kubernetes continuation belongs to one collection
snapshot, and version-selection parameters belong only to the first request.

**Declaration (author writes):** Kubernetes,
`examples/kubernetes-sdk/src/api.rs:104-105`.

Before:

```rust
GET pods(..ListOptions) -> PodList { path "pods"; }
```

After:

```rust
GET pods(..ListOptions) -> PodList {
    path "pods";
    page items response.items;
    page cursor request.continue_token <- response.metadata.continue_token, empty = end;
    page snapshot response.metadata.resource_version, empty = invalid;
    page initial request.resource_version, request.resource_version_match;
}
```

This design goes beyond F1's independent projections. The compiler describes a
**composite** continuation type containing token and snapshot, plus a first/next
request shape. `page initial` is the provider's allowed relationship: those
version selectors apply before a continuation exists. It is not a command to
clear arbitrary generated controls. Every other query value, including selectors,
limit and timeout, remains on the unchanged request template.

The Rust `SnapshotPager` materializes one allowed request shape at a time and
checks the declared invariant before returning records. It requires a nonempty
snapshot whenever an input or output continuation exists; it compares the
snapshot even on the terminal continued chunk. Empty items with a continuation
remain nonterminal. These are generic enforcement of the stated relation; there
is no traversal/restart/recovery program in the declaration.

Use a shared named **page-contract declaration** if this is adopted across the
13 current list endpoints; the endpoint still explicitly references that
contract. Do not put these facts on `core_v1`, where version, individual get and
watch operations are not chunked lists. The first experiment should use only
the shown endpoint rather than inventing another grouping language in advance.

**Call site (caller writes):**

```rust
// Before: existing public factory shape, src/list.rs:235-238
let pager = ListPager::new(ns.core().pods(), size);
let first = pager.fetch(None).await?;

// After: still an explicit Rust factory, not an automatic traversal
let pager = concord_pagination::SnapshotPager::new(
    ns.core().pods().set_limit(size.get()),
);
let first = pager.fetch(None).await?;
```

The caller can still drive the existing sequential engine with a page budget.
`fetch(Some(key))` retains the key's snapshot; the factory never substitutes a
fresh snapshot, consumes `remainingItemCount` as a plan, or follows the
inconsistent continuation offered by a 410. Error-document handling and
the decision to restart after expiration stay in the SDK/application.

**SDK code that disappears:**

- `examples/kubernetes-sdk/src/list.rs:119-153`: 35 lines defining, accessing and
  redacting Continuation become a generated typed composite key.
- `:270-279`: 10 lines of token replacement/version-selector construction become
  generic request materialization against the declared relation.
- `:303-328`: 26 lines of extracting items, checking snapshot/token and building
  Page become the generic relation checker/projector. Map its typed errors back
  to the existing ListError once in Rust, roughly 8–12 lines.
- Do not count `ListError::from_concord` at `:189-197`, API models, selectors or
  watch logic. The handwritten `ListOperation` macro at `:90-117` is a separate
  request-trait problem; F3 alone does not remove it without also changing that
  interface.

About 71 gross lines, roughly 60 after a shared error mapper, disappear from the
primary adapter. Declarations grow only for participating lists. This also
removes E's separate “paged” classification and B/G's composite-page projection
and context wrapper, rather than introducing a retry/runtime framework.

**Hardest counterexample:** **Kubernetes:** a token without a snapshot fails;
changing or losing the snapshot on any continued response fails before items
are accepted. A 410's alternate token never silently changes consistency.
**Elasticsearch:** its PIT ID **changes** rather than remaining invariant. Do
not annotate it `snapshot` or force it into a token-only key. Keep
`src/pit.rs:443-453` in Rust so the newest ID is retained even on a partial page,
and keep partial/shard, size and raw sort-value validation. Supporting that
case would need a separately justified evolving-state contract, not one more
meaning assigned to this snapshot keyword. **Discord:** ordering checks remain
independent evidence and are not removed by a snapshot key.

**Simple control (Nager):** Unchanged; no page key, invariant or first/next shape.

**Charter check:**

- C1: token/snapshot/first-request relationships are provider contract facts.
- C2: declarations describe relationships; Rust performs fetches, validation and restart decisions.
- C3: ordinary endpoints and all three controls acquire no page statements.
- C4: initial-only fields are explicit; no guessing from names like `resourceVersion`.
- C5: bad selectors and conflicting cursor/initial roles fail at their authored references.
- C6: docs say “Continuation bound to resourceVersion; version selectors only on the first chunk.”
- C7: native request inspection/execution remains available, with no consistency claim after bypass.
- C8: no implicit restart, prefetch, parallel requests or async drop.
- C9: wire-test initial/continued parameter shapes; mutate missing/changed terminal snapshots,
  empty intermediate pages, repeated tokens and 410 alternate tokens independently.

**New concepts:** Composite generated page key, `page snapshot`, `page initial`,
Rust `SnapshotPager`, typed invariant errors. A shared page-contract declaration
would be a further concept to evaluate, not assumed free.

**Breaking changes:** New facilities are additive. Replacing Kubernetes'
`Continuation` and `ListPager` changes named key/factory types and error mapping;
preserve token/snapshot accessors and explicit restoration semantics or provide a
versioned migration. Unlike F1, this is not a zero-cost syntax extension for an
already complex SDK.

**Why it might be wrong:** First/next request shapes plus invariant keys may be
too much language for one strong witness; the shared contract adds still more surface.
It deliberately does not solve evolving PIT state, so “general pagination” would be an inaccurate name.

## F4. One traversal constructor takes the start position; no language change

**Fact it expresses:** Starting and resuming a DigitalOcean listing are the same
provider operation with a different initial position, not two operations.

**Declaration (author writes):** DigitalOcean,
`examples/digitalocean-sdk/src/lib.rs:39`.

Before:

```rust
GET list(..Paging, query tag_name?: TagName) -> Droplets;
```

After, exactly unchanged:

```rust
GET list(..Paging, query tag_name?: TagName) -> Droplets;
```

This is a deliberately smaller alternative to a new declaration feature:
standardize the SDK-facing Rust method as
`droplet_pages(size, from: Resume<NextLink>, cap)`. Rename the existing `_from`
implementation and delete the start-only delegator. Do the same for other
traversals. There is no new `starting_at` mutation on an already running Pages,
no new operation identity, and no possibility of changing the position after
records have been read.

**Call site (caller writes):**

```rust
// Before
let first = api.droplet_pages(size, cap);
let resumed = api.droplet_pages_from(size, cap, from);

// After
let first = api.droplet_pages(size, Resume::Start, cap);
let resumed = api.droplet_pages(size, from, cap);
```

Mailgun preserves its eager Result:

```rust
// Before
let resumed = events.pages_from(from, cap)?;

// After: validation is still inside the constructor, before it returns.
let resumed = events.pages(from, cap)?;
```

**SDK code that disappears:**

- `examples/digitalocean-sdk/src/pagination.rs:28-30,46-53,68-75,90-97`:
  four start-only delegators, 27 physical code lines. Rename the `_from` bodies
  at `:36-43,56-65,78-87,100-109`; do not delete the actual paging implementation.
- `examples/notion-sdk/src/client.rs:137-139,167-169`: six lines of start wrappers.
- `examples/datadog-events-sdk/src/client.rs:154-156`: three lines.
- `examples/mailgun-sdk/src/events.rs:252-254`: three lines.
- `examples/microsoft-graph-sdk/src/paging.rs:183-185`: three lines.
- `examples/github-sdk/src/paging.rs:389-391`,
  `examples/stripe-sdk/src/pages.rs:419-421`, and
  `examples/discord-rest-sdk/src/api.rs:129-131`: nine more lines.

That is 51 physical code lines across 12 observed pairs, plus now-redundant
method docs; estimates exclude surrounding attributes/prose. Nothing here removes
the cursor facts still encoded in SDK adapters. It removes most **twin-method
ceremony**, not most of the inventory's 4,006 paging lines. F1 is the small option
when removal of actual projection glue is the objective. This option chiefly
benefits G/H: one caller method and one set of resume docs per traversal.

**Hardest counterexample:** **Mailgun:** renaming the existing `_from` body keeps
`src/events.rs:263-271` exactly where it is, preserving synchronous context
validation. A generic `Pages::starting_at` setter would not automatically preserve
that timing, which is why this version chooses an initial argument instead.
**Kubernetes, Elasticsearch and Discord:** all their validators and state remain
unchanged. If a continuation fails after records were accepted, the existing
Pages prefix/failed-key behavior is unchanged; the caller does not rerun the
start-only method to recover.

**Simple control (Nager):** Unchanged. Neither a single finite result nor manual
PokéAPI link following is converted into a traversal.

**Charter check:**

- C1: no declaration word is added to configure generated surface.
- C2: this only simplifies Rust construction; missing cursor facts still need F1/F2/F3.
- C3: no declaration or constructor change for any simple control.
- C4: Start and At are explicit existing Resume variants; no default is inferred.
- C5: Rust reports wrong cursor types at the constructor; runtime provider validation is preserved.
- C6: one method, one argument explanation; remove duplicated H09c prose.
- C7: request templates and all native escape paths remain untouched.
- C8: no changed validation/network timing, retries or hidden position mutation.
- C9: compile representative start/resume callers; retain Mailgun's synchronous-refusal test
  and existing cap/failed-key/ordering/snapshot regressions without rewriting their expectations.

**New concepts:** None. Existing `Resume` becomes a parameter of the single Rust
traversal method. No macro/core work is required.

**Breaking changes:** Every start-only traversal caller must pass `Resume::Start`;
every `_from` caller changes the method name, and DigitalOcean also normalizes
argument order to position before cap. Named function pointers break too.
Deprecated delegators can stage migration, but are not counted as removed while
they remain. This is a consciously chosen SDK surface break, not a compiler feature.

**Why it might be wrong:** It asks first-page callers for an extra value and
does not move a single missing cursor-location fact into the declaration.
The saved glue may not justify a broad source migration unless paired with a substantive paging improvement.

## Choosing experiments without conflating mechanisms

E1 is the first experiment: prove POST-read classification and unchanged simple
controls with a descriptor addition. E2 is an alternative extensibility mechanism,
not a reason to ship both a large fact schema framework and dozens of dedicated
keywords. E3 addresses a different deficit: the relation between a key and the
request actually dispatched; it composes with either effect representation.

For paging, F1 is the first experiment on Square and Intercom, with Notion's
incomplete-query check retained as an adversarial fixture. F2 separately earns
its extra dispatch/context abstraction against Mailgun, DigitalOcean, GitHub
and Graph. F3 must justify its additional vocabulary against Kubernetes without
pretending Elasticsearch is the same protocol. F4 can be measured independently
as a small public-surface simplification.

Do not adopt the seed “always generate the enum” as the solution by itself:
it removes a statement but leaves the fact tables. Do not introduce a
`#[retryable]`/`#[uncertain]` policy language, derive semantics from POST/GET,
or turn an operation identity into an idempotency proof. Preserve the caller's
reqwest client and the distinction between generated `send` and native
`prepare()?.send()` throughout.

These are source-grounded proposals with proposed acceptance tests. No compiler,
wire or canary test of the new syntax is claimed to have run, and the deletion
estimates are candidate migrations rather than achieved scoreboard reductions.
