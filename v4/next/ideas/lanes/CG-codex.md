# C / G — construction and the public SDK surface

Provider: codex. Read-only source baseline: `ba10189b` in `work/wt/rv-p1`.
All source locations below are relative to that repository. No implementation is proposed as
already working: **before** excerpts are current code; **after** excerpts are candidate language
or API designs, not compiled examples. Documentation comments are omitted from some excerpts.

There are **three designs per family**. C1 changes how a base enters a client; C2 makes related
facts one replaceable value; C3 keeps editable handles and checks the destination contract at
execution. G1 removes redundant public synonyms without adding syntax; G2 moves validation to
the input type; G3 gives a provider operation a generic output. These are different mechanisms,
not three spellings of `controls`. They may compose; their savings must not be added where they
delete the same shell.

## Evidence and accounting

The charter and `v4/next/inputs/CONSTRUCTION_PROBLEM.md` are the acceptance criteria. The draft
inventory reports **1,242 construction** and **829 forwarding** nonblank lines across all SDKs;
its 21,361 total glue lines also include other families. Those are heuristic labels, not a
promise that 2,071 lines can safely be deleted. The friction catalogue at the supplied path was
absent; I used the witness SDKs' `FRICTION.md` and available `MAINTENANCE.md` instead. Historical
notes sometimes describe code that has since been replaced; the locations below refer to the
current source.

The required first step for G is causal classification. Here is the source-based disposition,
including mixed methods that the classifier calls forwarders:

| SDK and current source | Forwarding cause | What must remain / plausible removal |
|---|---|---|
| Mastodon, `src/lib.rs:119-175`, `:178-239` | Construction and naming; streaming also provider semantics | REST delegates and host-installation shell can go with C1. Discovery, advertised-host binding and downgrade checks stay in Rust. |
| Shopify, `src/context.rs:28-97`, `:109-205` | Construction, duplicate context; served-version semantics | C2 can remove duplicate shop/version storage. Served-version refusal and credential-free signed downloads stay. Merely renaming endpoints removes neither. |
| Zendesk, `src/lib.rs:108-131` | Construction: bearer conversion and default policy | This is a factory, not an endpoint forwarder. Validation and retry behavior remain Rust; initialized policy support is an independent small opportunity. |
| Salesforce, `src/query.rs:63-153` | Output-type dispatch, rather than naming | G3 removes first/more dispatch. Record attributes, record IDs, next-link validation and page projection remain. |
| QuickBooks, `src/lib.rs:157-168`; `src/policy.rs:32-62` | Construction and base/connection synchronization | Realm and credentials are already one `Connection`. C2 should extend this success to the base; refresh/replay decisions remain. |
| Xero, `src/lib.rs:63-121`, `:142-243`, `:337-466` | Construction, validation, paging and provider semantics | Group tenant with its credential journal using C2. Cross-tenant references, continuation checks and uncertain-write classification are not pure forwarders. |
| DigitalOcean, `src/lib.rs:56-108`, `:122-184` | Construction and result shape; pagination elsewhere | Origin/fixture construction is C; value-plus-metadata is B. Keep transport policy and validation of followed links. |
| Datadog, `src/client.rs:69-82`, `:121-146`, `:154-197` | Construction; pure naming delegates; paging | G1 removes the naming delegates; C2 can remove key/site unpacking. Cursor guards and warnings survive. |
| Discord, `src/api.rs:36-49` and `src/limiter.rs` | Credential/limiter coupling; admission behavior | One bot session is C2. Current `query(..) anchor` already removed an older cursor facade; do not count that historical wrapper again. |
| PayPal, `src/lib.rs:176-281`; `src/policy.rs:195-216` | Construction and destination protection | Builder constructs private `Session`; it is not nine useless forwarders. C3 can remove the repeated destination guard, not token acquisition or replay policy. |
| Registry, `src/api.rs:7-22`; `src/client.rs:116-124`, `:151-223` | Construction/path expressiveness, auth and provider semantics | Repository rebasing works around multi-segment names. Challenge replay, digest verification and server upload locations stay. Neither aliases nor generic outputs solves this. |
| Drive, `src/client.rs:145-170`, `:203-261` | Validation, typed-input lowering, result shape and paging | G2 removes input adapters after a shared send policy preserves existing results. Incomplete search, boundary-token exclusivity and repeated-token checks remain. |
| OpenAI, `src/lib.rs:346-419`, `:421-458` | Body-mode semantics, validation, result/stream adaptation; one pure delegate | `create_batch` is pure naming. `create_response` fixes `stream=false`; streaming opens a reader; content enforces a caller's bound. Polling and combining batch files are behavior. |
| Elasticsearch, `src/lib.rs:182-263`, `:271-299` | Validation, response correlation and PIT lifecycle | ID checks, bulk correlation and PIT bookkeeping are behavior. Its 79-line inventory excerpt is not 79 removable lines of naming. |

Paths in that table have the prefix `examples/<SDK>-sdk/`. Source review also covered the
counterexamples `amazon-s3-sdk/src/{api,client}.rs`,
`kubernetes-sdk/src/{api,context}.rs` and `clickhouse-sdk/src/{wire,client}.rs`.
S3's former 36 builder forwarders are already gone; current signing and credential rotation
must be preserved. Kubernetes has a useful unauthenticated default and proxy mode. ClickHouse
has useful defaults and a credential enum, not an always-required bearer token.

For every design, “Simple control” means all three controls: Nager
`src/lib.rs:15-39`, PokéAPI `src/lib.rs:70-88`, Frankfurter `src/lib.rs:19-54`.
Their declarations must remain byte-for-byte unchanged, including their existing send policies.
Estimates below use physical source lines including nearby docs, explicitly not the inventory's
nonblank-line metric. “Removed” excludes code merely relocated into a model or behavior function.

# Family C

## C1. A constructor-owned typed base — smaller than you think

**Fact it expresses:** A Mastodon client belongs to the instance supplied at construction.

**Declaration (author writes):**

Before — `examples/mastodon-sdk/src/api.rs:16-25`:

```rust
pub client Api(
    #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
) {
    base "https://mastodon.invalid";
    send MastodonPolicy -> Result<_, Error>;
    format Json;
    // Existing endpoints follow.
}
```

After — proposed typed-base form; rename the declared client to the public SDK name:

```rust
pub client Mastodon(
    #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
) {
    base instance: InstanceUrl;
    send MastodonPolicy -> Result<_, Error>;
    format Json;
    // Existing limits, parameter group and endpoints are unchanged.
}
```

`base instance: InstanceUrl;` contributes a required `instance` field to `MastodonConfig`.
`InstanceUrl` exposes an already-validated `concord::BaseUrl` through
`AsRef<concord::BaseUrl>`. The compiler stores that typed input, rather than a placeholder and
an independently replaceable base. This form has no `with_base_url` or `set_instance`.
The existing literal-base form keeps its present behavior, so unauthenticated simple clients
and useful default deployments do not acquire new concepts.

Expose an immutable typed `context()` view of the stored facts and `http()` for Rust
extensions. Its `Debug` must apply the declaration's sensitivity, not derive raw field output.
Raw secret access is explicitly named, e.g. `authorization_unredacted()`; it is never an
implicit logging operation. This is needed to retain `discover_streaming` and the three
stream builders as ordinary inherent Rust methods on the generated client. Their trust check
runs before constructing a fresh client for the advertised streaming origin and explicitly
re-supplying its credential. There is no public operation that silently moves the old credential.

**Call site (caller writes):**

```rust
// Before: the handwritten Mastodon wrapper.
let api = Mastodon::new(http, InstanceUrl::parse(instance)?).with_token(&token);
let page = api.public_timeline().send().await?;

// After: the generated Mastodon, plus the existing Rust with_token extension.
let api = Mastodon::new(http, MastodonConfig {
    instance: InstanceUrl::parse(instance)?,
}).with_token(&token);
let page = api.public_timeline().send().await?;
```

`with_token` still replaces an optional header snapshot for the same instance. Its body remains
the existing `set_authorization` call. This design deliberately does not change ordinary value
setters.

**SDK code that disappears:**

- `examples/mastodon-sdk/src/lib.rs:119-140`: wrapper storage and placeholder-base constructor,
  22 lines; no more “validated URL nevertheless might fail” `unreachable!`.
- `examples/mastodon-sdk/src/lib.rs:157-175`: four REST delegations, 19 lines.
- `examples/mastodon-sdk/src/lib.rs:242-247`: wrapper-only `Debug`, 6 lines.
- `examples/mastodon-sdk/src/api.rs:21-23`: placeholder explanation/base, 3 lines replaced by
  one real fact.
- Gross removal approximately **50 lines**. Keep `InstanceUrl`'s stricter origin validation
  (`src/model.rs:26-32`, `:47-67`), request aliases needed for compatibility, discovery and
  stream trust logic. Add an `AsRef<BaseUrl>` implementation and adapt the Rust extensions;
  expect materially less than 50 net lines. Do not count moved extension bodies as deletions.

This removes **C construction, G forwarding and H placeholder documentation** together.
It handles the origin half of Salesforce as well, but not its version-range invariant.

**Hardest counterexample:** **QuickBooks**. An immutable independent base is insufficient:
`set_connection` may change sandbox/production together with realm and credential. Keep its
current `select_environment` behavior until C2 is adopted; do not freeze `Connection` to make
this test pass. S3 and Kubernetes retain credential rotation; Kubernetes and ClickHouse retain
literal defaults or use an initialized typed base only if they opt in.

Fixtures construct a fresh client using a validated fixture base and a synthetic credential.
A `cfg(test)` constructor alone is insufficient for integration tests; an explicit Rust fixture
constructor or opt-in fixture feature must also work for consumers. Provider-specific “loopback
only” rules stay in the SDK type, not in this syntax. A reqwest proxy needs no destination
change; a reverse proxy is an explicit new instance/base at construction.

**Simple control (Nager):** Unchanged; PokéAPI and Frankfurter also unchanged.

**Charter check:**

- C1: The supplied deployment/instance is a provider fact; no generation-control list.
- C2: Base ownership is declared; validation, discovery and trust decisions are Rust.
- C3: Existing literal bases and all three control declarations keep their shape.
- C4: A named base input visibly identifies where the URL comes from.
- C5: A non-base type is diagnosed at `InstanceUrl`; invalid URLs fail in its constructor without echoing input.
- C6: “Supply the instance once when constructing the client” is the whole caller rule.
- C7: The caller's reqwest client, `prepare`, `into_parts` and `send_raw` remain available.
- C8: Construction stores a value; no discovery, auth refresh or I/O occurs implicitly.
- C9: Require compile-fail tests for rebasing this form, two-origin credential-canary tests, same-instance rotation/snapshot tests, and unchanged default/fixture wire tests.

**New concepts:** Named typed `base` input; public `BaseUrl`; immutable typed context access.
`BaseUrl` is a checked URL value, not a new transport.

**Breaking changes:** Adopting SDK's constructor/config and generated type names change;
`with_base_url` is unavailable on that opted-in form; request aliases can remain ordinary Rust
compatibility exports. Macro collision checks must include new context accessors.

**Why it might be wrong:** It removes independent rebasing, not the ability to deliberately
construct a new client with a wrong credential. Mastodon's discovered second origin also proves
that “one host forever” would be too restrictive; its Rust trust check remains necessary.

## C2. One validated context, projected atomically into all request facts

**Fact it expresses:** A QuickBooks accounting connection selects one realm, credential source and environment together.

**Declaration (author writes):**

Before — `examples/quickbooks-online-sdk/src/lib.rs:67-80`:

```rust
pub client QuickBooks(
    connection: Connection,
) {
    base "https://quickbooks.api.intuit.com";
    path "v3/company/{connection}";
    query { "minorversion" = "75"; }
    request Json;
    response QboJson;
    limit ENTITY_BODY_LIMIT;
    send connection -> Result<_, Error>;
}
```

After — a validated Rust record supplies several facts; selectors are stored fields:

```rust
pub client QuickBooks(
    context account: AccountingContext,
) {
    base account.base;
    path "v3/company/{account.realm}";
    query { "minorversion" = "75"; }
    request Json;
    response QboJson;
    limit ENTITY_BODY_LIMIT;
    send account.connection -> Result<_, Error>;
}
```

`AccountingContext` is an SDK-owned, `Clone` input record with private `base: BaseUrl`,
`realm: RealmId` and `connection: Connection` fields. A Rust constructor takes **one**
`Connection` and obtains the first two from it. Declare/implement this record in the same Rust
module as the macro so ordinary privacy permits the projections. It cannot be created from an
independent realm, base and token by public struct literal or unchecked deserialization.

`context` identifies an indivisible provider context: generate `set_account`, never
`set_realm`, `set_base` or `set_account_connection`. Its projected base has no independent
`with_base_url`. One replacement changes routing and policy in a single copy-on-write snapshot;
already-created endpoints retain the old record. Field projections also work in headers,
query placements and `send`. They are typed field access, not callbacks or arbitrary expressions
in template strings. Nested field projection is allowed; indexing, branching and calls are not.

To avoid allowing low API versions through a constructor, Salesforce's context record must
contain a `SupportedApiVersion` built by a checked Rust constructor. Merely declaring
`ApiVersion` is insufficient: its public `new` currently accepts versions below `MINIMUM`.
The minimum check is validation in Rust, not an extra comparison language in the DSL.

**Call site (caller writes):**

```rust
// Before; lib.rs:162-166, then the currently generated replacement.
let api = QuickBooks::connect(http, connection_a);
let old = api.customer(id.clone()).read();
let api = api.set_connection(connection_b);

// After: the public input proves the associated facts once.
let api = QuickBooks::new(http, QuickBooksConfig {
    account: AccountingContext::new(connection_a),
});
let old = api.customer(id.clone()).read();
let api = api.set_account(AccountingContext::new(connection_b));
// old still names account A; future requests name account B, including its environment.
```

The same mechanism can express Datadog `context account: DatadogAccount` with projections to
`site.base` and both keys; and Discord `context bot: BotSession` with token and limiter
projections. Their validated Rust constructors must preserve the actual invariant: Datadog's
two keys are checked together, and Discord creates/shares a limiter belonging to that bot.
A freely constructible tuple of unrelated values would provide atomic assignment but no proof.

**SDK code that disappears:**

- `examples/quickbooks-online-sdk/src/lib.rs:157-168`: base-installing `connect`, 12 lines,
  if callers use the generated constructor. A compatibility factory may remain temporarily.
- `examples/quickbooks-online-sdk/src/policy.rs:35-37` and `:48-62`: stale-base explanation,
  invocation and official-host rewrite, 18 lines. Preparation now selects the correct
  environment before signing/authentication, with no URL surgery.
- About **30 gross lines**, offset by approximately 12–20 lines for the Rust context record,
  constructor and fixture constructor. Refresh, caching and read/write behavior are unchanged.
- Additional opportunity: `examples/datadog-events-sdk/src/client.rs:69-82` (14 lines) no
  longer unpacks credentials into two raw fields and installs a site URL. Keep a small validated
  `DatadogAccount` constructor; do not count that moved work as removed.
- Shopify `src/context.rs:90-97` (8 lines) can stop assembling a second shop representation
  when its installation is one scope context. The served-version and download behavior still
  needs Rust and is not included in savings.

This is the broad candidate for **C + G**, with **I** gains from typed inputs. Typed access to
`account.realm` also avoids treating a public realm ID as secret merely because the whole
`Connection` is the send policy, as happens today. Sensitivity belongs on individual projections;
`account.connection` remains entirely withheld.

**Hardest counterexample:** **QuickBooks** is the primary witness, not an exception. A
production-to-sandbox replacement must change origin, realm and policy even when only
`prepare()` is called. Fixture routing must be explicit: `AccountingContext::for_fixture`
creates a complete fixture context, and replacing it with `new(connection_b)` selects B's
official origin. Keeping a fixture across replacement requires an explicit new fixture context.

S3 uses a validated signing context containing region, credentials and clock. Rotating
credentials for the same region constructs a replacement using the existing Rust
`with_credentials` behavior; it does not generate `set_region` next to a stale signer.
Kubernetes can keep its current optional bearer and in-cluster default; opting into a cluster
context must retain an unauthenticated/default constructor. ClickHouse's no-auth and Basic /
header credential alternatives remain a Rust enum. Proxies remain caller-owned reqwest
configuration; a reverse-proxy base belongs to the validated whole context.

PayPal's private `Session` factory is still real behavior. This mechanism does not make its
private constructor public or erase its builder; it can make the resulting session and API base
one context. Do not claim that a newly generated `new` automatically solves its unusable
public-config documentation.

**Simple control (Nager):** Unchanged; no implicit `context` is introduced in any control.

**Charter check:**

- C1: One provider connection supplies associated realm, destination and credentials.
- C2: Placement is declared; context validation and credential acquisition stay in Rust.
- C3: New context bindings are opt-in; ordinary required/optional/default values retain current syntax.
- C4: Explicit field selectors show exactly which fact supplies each wire contribution.
- C5: Missing/private/incompatible projected fields point to the written selector; duplicate contributions retain existing diagnostics.
- C6: “Replace the whole connection, never one of its parts” is the caller rule.
- C7: Contexts contain no transport replacement; all existing native escape paths survive.
- C8: A projection reads stored data, once per snapshot; refresh and rate state change only in explicit Rust behavior.
- C9: Require compile-fail tests for individual projected setters and invalid context construction; prepared/wire tests for account/env replacement, retained old handles, rotation and redaction of only secret projections.

**New concepts:** `context` parameter; typed field projections in placements/templates/`send`;
SDK-owned validated context records. No tuple-update DSL or refresh engine.

**Breaking changes:** Opted-in configs and setters take a whole context; callers lose individual
projected setters and independent rebasing. Generated policy selection must support a projected
field while preserving `SendWith<T>` / `SendOperation<T, Op>`. No change to ordinary endpoint
setters or to non-adopting declarations.

**Why it might be wrong:** A group alone cannot prove that its members belong together; the
public Rust constructors are part of the guarantee and must be tested. If each SDK needs a
large projection-only record, this relocates construction glue instead of eliminating it.

## C3. Declare the credential audience; reject a moved request at execution

**Fact it expresses:** A PayPal session's credentials are valid only at its configured origin.

**Declaration (author writes):**

Before — `examples/paypal-sdk/src/lib.rs:63-73`:

```rust
pub client PayPal(
    session: Session,
) {
    base "https://api-m.sandbox.paypal.com";
    format Json;
    limit BODY_LIMIT;
    send session -> Result<_, Error>;
    operations PayPalOperation;
}
```

After — retain the current construction/setter shape, add a positive credential fact:

```rust
pub client PayPal(
    session: Session,
) {
    base "https://api-m.sandbox.paypal.com";
    audience session;
    format Json;
    limit BODY_LIMIT;
    send session -> Result<_, Error>;
    operations PayPalOperation;
}
```

`Session: CredentialAudience` supplies immutable **data**, a set of exact allowed origins.
For this SDK the set has one element, from `Session`'s existing base. It does not supply a
`permits(request)` callback, a retry decision, a URL matcher or an authentication action.
The generated prepared request/decoder snapshots that fact and its declaration identity.
All Concord execution routes check the final request destination before handing it to reqwest.

This is an alternative to suppressing construction methods. `with_base_url` and `set_session`
can still be used, but a mismatched pair cannot execute through Concord. Check at
`Decoder::execute` / the shared execution boundary as well as direct `Prepared` sends:
a policy may rewrite a prepared request, as QuickBooks does. Do **not** reject only in
`prepare()`, before the policy has selected its final destination. The error is a redacted
`Build` / `Delivery::NotSent` error naming the `audience` fact.

A fixture session is constructed with the fixture's origin. A fresh context must explicitly
authorize a reverse proxy. Ordinary reqwest forward proxies preserve the request target and
need no audience edit. This mechanism cannot police redirects that a caller-configured reqwest
client follows internally; existing no-redirect transport requirements remain essential.

**Call site (caller writes):**

```rust
// Before: policy.rs:208-210 returns Error::Destination after prepare.
let moved = paypal.clone().with_base_url(other)?;
let failure = moved.order(id.clone()).get().send().await.unwrap_err();

// After: same expression; a core destination-contract Build error, before API I/O.
let moved = paypal.clone().with_base_url(other)?;
let failure = moved.order(id.clone()).get().send().await.unwrap_err();

// A legitimate fixture still builds a matching API/session pair in ordinary Rust:
let fixture = PayPal::builder(http, credentials).base_url(server.url()).build()?;
let order = fixture.order(id).get().send().await?;
```

**SDK code that disappears:**

- `examples/paypal-sdk/src/policy.rs:208-210`: the repeated origin comparison and return,
  **3 lines**. The surrounding token/retry policy remains.
- The builder's base propagation at `src/lib.rs:269-280` does **not** disappear under this
  design; there are still two values to construct. The data-only trait implementation adds
  approximately 4–8 lines here. This is a shared safety primitive with little or negative
  immediate net reduction in PayPal, not a credible claim to erase its builder.
- Other SDKs could remove equivalent hand-written destination checks after declaring the fact,
  but this document does not assign an unverified line count to them.

This moves a repeated **C invariant into a core check** and can support **J redacted diagnostics**.
It is a weaker glue-reduction candidate than C1/C2, included because it preserves the editable
surface and checks final native edits rather than relying only on absent setters.

**Hardest counterexample:** **QuickBooks**. Its current policy can keep rewriting either
official host to the connection's environment; the final execution check uses the selected
connection's audience. `set_connection` remains atomic for realm/auth as it is today.
An arbitrary rebase is refused; fixtures need an explicitly fixture-authorized connection.
The existing production fixture exception must not turn into “all nonofficial hosts allowed.”

For Mastodon, Rust discovery may produce an audience including a separately trusted streaming
origin; the type must record which instance authorized it. A caller-supplied URL alone is no
proof. S3 credentials can authorize many endpoints, so do not infer an origin restriction for
every secret or force this mechanism onto S3/Kubernetes. Salesforce FM4 still needs a
validated supported-version type; an origin check does not fix it.

**Simple control (Nager):** Unchanged; there is no audience check without an audience fact.

**Charter check:**

- C1: Credential destination is a provider fact; no `remove with_base_url` directive.
- C2: The declaration names audience data; acquiring or enlarging it requires ordinary Rust validation.
- C3: No new lines, traits or checks for any control.
- C4: Exact origins mean scheme, host and effective port; no inferred suffix trust.
- C5: Contract errors name `audience session` and the operation, without rendering credentials or full URLs.
- C6: “This session may be sent only to its audience” is the complete rule.
- C7: Native reqwest execution after `into_parts` remains an explicit escape, outside this guarantee; `send_raw` still checks destination.
- C8: One deterministic local check; no middleware, redirects, refresh or retries are installed.
- C9: Require a two-server trap test for every Concord send route, mutation-after-prepare tests via `Decoder::execute`, fixture/proxy tests, rotation snapshots and canary/error-format tests.

**New concepts:** `audience`; data-only `CredentialAudience`; exact-origin audience data
carried by `Prepared`/`Decoder`. No arbitrary predicate language.

**Breaking changes:** Opted-in SDK failures change from `Error::Destination` to an underlying
Concord Build error unless a compatibility conversion is kept. Sending a formerly accepted
arbitrary rebase now fails. Native escape semantics must be stated precisely.

**Why it might be wrong:** The guard cannot see reqwest's internal redirects, and a raw client
can bypass it after `into_parts`. It also preserves contradictory configurations until send time;
if structural construction solves the same cases more simply, prefer C1/C2.

# Family G

## G1. Keep the generated tree; delete pure public synonyms — smaller than you think

**Fact it expresses:** Datadog's list, search and post operations belong to its events resource.

**Declaration (author writes):**

Before — exact endpoint facts from `examples/datadog-events-sdk/src/api.rs:60-68`, inside
`scope events`:

```rust
POST search(body request: SearchRequest) -> EventsPage { path "v2/events/search"; }

POST post(body event: NewEvent) -> EventAcceptance {
    path "v1/events";
    request PostJson;
}
```

After — **the declaration stays identical**:

```rust
POST search(body request: SearchRequest) -> EventsPage { path "v2/events/search"; }

POST post(body event: NewEvent) -> EventAcceptance {
    path "v1/events";
    request PostJson;
}
```

Existing `send SendPolicy -> Result<_, Error>;` already makes these generated operations return
the SDK result. No alias stanza, flattened endpoint registry or facade-generation directive
is necessary.

The public SDK may keep its construction wrapper during a migration. Its existing `api()`
method (`src/client.rs:117-119`) already hands out the generated client, so removing convenience
synonyms does not require inventing a second route. After C2 makes construction safe, publish
the generated client itself with an ordinary Rust `pub use`. Existing public request type
exports can be simplified with `pub use api::*;` where that module contains only the intended
surface; otherwise keep explicit Rust exports. Import layout is Rust packaging, not a provider
fact to encode in `api!`.

**Call site (caller writes):**

```rust
// Before:
let accepted = client.post(event).send().await?;
let page = client.search(request).send().await?;

// After, even while the construction wrapper remains:
let accepted = client.api().events().post(event).send().await?;
let page = client.api().events().search(request).send().await?;

// After safe direct construction (C2), drop only the api() hop:
let accepted = client.events().post(event).send().await?;
```

**SDK code that disappears:**

- `examples/datadog-events-sdk/src/client.rs:121-146`: `events`, `list`, `search` and `post`
  delegations, **26 lines**, including docs. The former `list(range)` becomes
  `api.events().list().set_range(range)`; range validation stays in `TimeRange`.
- `examples/openai-sdk/src/lib.rs:411-413`: pure `create_batch` delegation, **3 lines**;
  callers use existing `OpenAiApi::batch(request).send()`. Exposing a safe generated OpenAI
  entry point is a construction migration, not a saving attributed twice here.
- Approximately **29 gross lines** in these two concrete examples. No replacement logic is
  necessary. A deprecated compatibility delegate delays, rather than realizes, that deletion.

This retires **G naming glue** without adding a DSL concept, and composes with C. It does
**not** remove OpenAI's `create_response` (sets a body field), `stream_response` (opens and
checks the stream), `content` (runtime bound), or Datadog's search traversal.

**Hardest counterexample:** **Elasticsearch**. Its `get_document` delegate also compares the
returned ID (`src/lib.rs:191-197`); `bulk` correlates each result and `open_point_in_time`
constructs a stateful domain handle. Keep those behaviors. Registry's challenged auth, upload
locations and digest verification are the same kind of counterexample. A naming change cannot
turn their raw generated sends into equivalent public SDK operations.

**Simple control (Nager):** Unchanged, as are the other controls; their existing endpoint names
and navigation are already adequate.

**Charter check:**

- C1: Only current resource/endpoint facts remain in the declaration.
- C2: Rust behavior survives; a pure delegation is removed rather than restated elsewhere.
- C3: No new declaration or import requirement for the controls.
- C4: Navigation follows the written `scope events`, with no hidden flattening convention.
- C5: Existing compiler diagnostics and generated operation identities remain intact.
- C6: One navigation tree and one terminal `send`; no alias precedence rules.
- C7: Callers directly obtain generated handles and their native escape methods.
- C8: The same endpoint and send policy execute; no runtime adapter is added.
- C9: Require old/new prepared-request equivalence and caller compile tests; add a new endpoint and verify that it is reachable without adding a delegate.

**New concepts:** None. Ordinary Rust exports and the current `send` policy suffice.

**Breaking changes:** Callers of removed convenience methods navigate through the resource and
call `send`. Keep aliases/deprecated methods only if compatibility justifies delayed savings.
Do not rename the actual operation just to improve an inventory score.

**Why it might be wrong:** The extra navigation can be less convenient, especially while an
`api()` construction-wrapper hop remains. This addresses the pure residual naming category,
not most of the 98 historical methods when their bodies also implement behavior.

## G2. Validated inputs are the endpoint's public signature

**Fact it expresses:** Drive metadata retrieval takes a nonempty non-dot file ID and an explicit field selection.

**Declaration (author writes):**

Before — `examples/google-drive-sdk/src/api.rs:24-25`:

```rust
GET get(id: String, query fields: String) -> File { path "{id}"; }
POST create(body metadata: NewFile, query fields: String) -> File {}
```

After — ordinary existing Concord type syntax, no validation expression language:

```rust
GET get(id: FileId, query fields: FileFields) -> File { path "{id}"; }
POST create(body metadata: NewFile, query fields: FileFields) -> File {}
```

`FileFields` already exists (`src/model.rs:85-119`). Give it a `Display` implementation that
writes its stored wire selection; retain its existing “always include id” constructor.
`FileId` is a private-field Rust newtype; its constructor contains the SDK's present check
from `src/client.rs:205-207`. These are the current SDK's supported input rules, not a claim
to know the provider's complete identifier grammar. No unchecked `From<String>` or derived
`Deserialize` may bypass a constructor's invariant.

The client must also bind its existing generic finish behavior once:

```rust
// At the client, alongside existing base and format facts:
send policy -> Result<Response<_>, Error>;
```

Here `policy` is a required `DrivePolicy` client value. Its ordinary `SendWith<T>` impl does
what `DriveClient::finish` does now: status/error decoding and configured finite response limits.
This is existing Concord machinery; C1/C2 supplies safe construction. Do not simply expose
today's private `DriveApi`, whose current `send()` lacks that SDK policy.

Endpoint inputs/setters continue to accept the declared validated type. The constructor does
the validation once before the endpoint exists. The generated endpoint can then be public
without a wrapper that is the only place a raw string is checked.

**Call site (caller writes):**

```rust
// Before:
let file = client.get_file("file-7", &fields).await?;
let created = client.create_file(metadata, &fields).await?;

// After: client is the generated, policy-bound client.
let file = client.files().get(FileId::new("file-7")?, fields.clone()).send().await?;
let created = client.files().create(metadata, fields).send().await?;
```

For OpenAI, the same design declares `GET file(id: FileId)` and
`GET batch_status(id: BatchId)` instead of their current `String` inputs
(`src/lib.rs:53`, `:61`), then uses generated `send`. Its existing `SendPolicy` already
preserves the result shape. Distinct ID types prevent mixing file and batch IDs; do not claim
that a nonempty-string check verifies that an ID exists remotely.

**SDK code that disappears:**

- `examples/google-drive-sdk/src/client.rs:203-224`: `get_file` and `create_file` wrappers,
  **22 lines**, once the shared policy preserves existing semantics.
- The get check (`:205-207`, 3 lines) **moves** into `FileId::new`; it is not deleted behavior.
  Field-selection conversion (`:211`, `:222`) disappears as lowering glue because the
  declared value carries its wire representation.
- `examples/openai-sdk/src/lib.rs:396-401` and `:414-419`: two validating delegates,
  **12 lines**. Their checks move into ID models; their send delegations disappear.
- Thus **34 gross wrapper lines** are removed, but add the newtypes, constructors and
  `Display` implementations. Drive's generic `finish` body is relocated into the policy,
  with no claimed deletion. Net source lines may grow for a one-use ID; measure separately
  from the decline in facade duplication.

This connects **G validation, C typed construction and I input diagnostics**. Model redaction
from D can be reused, but neither plain `Display` nor a newtype automatically redacts secrets.
Keep `#[sensitive]` on token/cursor placements.

**Hardest counterexample:** **Xero**. A valid invoice UUID can still belong to another tenant.
Keep `Tenant::check`, `InvoiceRef` / `ContactRef` ownership and draft/continuation checks;
a plain `InvoiceId` newtype is insufficient. Likewise, Drive's mutually exclusive boundary
tokens (`src/client.rs:249-255`), repeated-token validation and incomplete-search error are
response semantics, not input-validation adapters to delete.

OpenAI `content(id, limit)` retains its per-call limit behavior. Registry's multi-segment
repository name remains a path-composition problem even after validation. This design must not
silently change the existing one-segment path encoding to make that example fit.

**Simple control (Nager):** Unchanged; do not invent `CountryCode`, `Year` or `Currency`
newtypes for the controls just to normalize the gallery.

**Charter check:**

- C1: The declaration names the provider/domain input, not a validation callback.
- C2: Validation and wire formatting live in Rust model types; placements remain declared.
- C3: Controls keep their current `String`/numeric inputs and declaration lines.
- C4: The declared type is exactly the public input; no inference from an identifier's spelling.
- C5: Invalid input fails at `FileId::new`; passing the wrong type points to the authored endpoint type.
- C6: “Construct a FileId, then pass it to get” needs no framework rule.
- C7: Generated operations retain preparation and native execution access.
- C8: Construction/formatting is local; no hidden lookup, request or credential refresh.
- C9: Require constructor and deserialization-bypass tests, compile-fail cross-ID tests, setter tests, and old/new request/result equivalence with the original bounds and error documents.

**New concepts:** SDK model types such as `FileId` / `BatchId`. No new DSL keyword or universal
`Validated` trait; use existing exact custom input types and formats.

**Breaking changes:** Raw strings become validated arguments; model construction may add a `?`
at the caller. Async convenience methods become generated builders plus `send`. Opening a
previously private generated client is conditional on preserving its policy and construction
invariants, not just changing `pub(crate)` to `pub`.

**Why it might be wrong:** Some wrappers accept raw values precisely to make callers' code
shorter, so the validation cost can move onto every caller. Newtype boilerplate can exceed the
removed lines; use this for repeated domain inputs and bypass prevention, not as a numeric trick.

## G3. One provider operation, generic over its record output

**Fact it expresses:** Salesforce SOQL returns records whose schema is chosen by the query.

**Declaration (author writes):**

Before — `examples/salesforce-sdk/src/api.rs:81-98`, inside `scope query`:

```rust
GET accounts(#[sensitive] query q: Soql) -> QueryResponse<Account>;
GET contacts(#[sensitive] query q: Soql) -> QueryResponse<Contact>;
GET dynamic(#[sensitive] query q: Soql) -> QueryResponse<DynamicRecord>;
GET more_accounts(#[sensitive] locator: QueryLocator) -> QueryResponse<Account> {
    path "{locator}";
}
GET more_contacts(#[sensitive] locator: QueryLocator) -> QueryResponse<Contact> {
    path "{locator}";
}
GET more_dynamic(#[sensitive] locator: QueryLocator) -> QueryResponse<DynamicRecord> {
    path "{locator}";
}
```

After — proposed endpoint type parameters using ordinary Rust bounds:

```rust
GET records<R: serde::de::DeserializeOwned>(
    #[sensitive] query q: Soql,
) -> QueryResponse<R>;
GET more<R: serde::de::DeserializeOwned>(
    #[sensitive] locator: QueryLocator,
) -> QueryResponse<R> {
    path "{locator}";
}
```

Both remain in the existing `scope query(batch_size?: BatchSize)`, with the same path and
header contributions. The output parameter becomes part of the generated endpoint type,
e.g. `SalesforceApiQueryRecords<R>`. It is not a runtime decoder swap and does not erase the
declared `QueryResponse` envelope.

The generated implementation is still `Endpoint<QueryResponse<R>>`; `send` policies and
`Sends<O>` see that concrete output. The endpoint's operation identity depends on the declared
provider operation, not on `R`: Account and Contact are two decodings of the same operation.
Its `Clone` depends on stored request inputs, not on whether `R: Clone`. Output-only type
parameters should use an appropriate non-owning phantom representation, and ordinary send/
decoder bounds must remain explicit.

**Call site (caller writes):**

```rust
// Before, query.rs:216-220 dispatches through QueryRecord:
let scope = ctx.api().query().maybe_batch_size(batch);
let request = match &key {
    None => R::first(&scope, soql.clone()),
    Some(locator) => R::more(&scope, locator.clone()),
};

// After, same traversal behavior with direct generic generated endpoints:
let scope = ctx.api().query().maybe_batch_size(batch);
let request = match &key {
    None => scope.records::<R>(soql.clone()).prepare(),
    Some(locator) => scope.more::<R>(locator.clone()).prepare(),
};

// A direct caller can also choose its own deserializable record:
let response = ctx.api().query().records::<MyAccount>(soql).send().await?;
```

This does not prove that the SOQL text selects `R`'s fields. A mismatched response remains a
decode error, just as it can be with today's Account-specific endpoint.

**SDK code that disappears:**

- `examples/salesforce-sdk/src/query.rs:65-83`: first/more dispatch requirements and docs,
  **19 lines**.
- `examples/salesforce-sdk/src/query.rs:93-104`, `:114-125`, `:135-146`: six first/more
  forwarding implementations, **36 lines**.
- Thus **55 physical glue lines** removed from the trait/impls. `QueryRecord`'s `attributes`
  and `id` methods, and their implementations, remain for response validation.
- Six declarations at `examples/salesforce-sdk/src/api.rs:81-98` become two, removing four
  duplicated operation facts (roughly another 10 lines after formatting). The generated-call
  change at `src/query.rs:218-219` replaces two dispatch calls; it is not extra savings.
- Keep `query.rs:243` onward: page/envelope consistency, returned-link checks and record
  correlation. Also keep error/usage adaptation around the prepared request.

This removes **G dispatch, B output duplication and L repeated declarations**. It also makes
a caller's record extension possible without editing the SDK. It is not an endpoint-alias
system; shared path and format facts still come from the existing scope.

**Hardest counterexample:** **PayPal** has `get() -> Order` and
`get_authorized() -> AuthorizedOrder` for the same Show-order endpoint
(`src/lib.rs:99-104`). Generic decoding could consolidate them, but retain the named views
when they communicate useful provider semantics or preserve public struct-literal compatibility.
Do not require every operation to become generic.

Registry manifest bytes are hashed exactly, and Elasticsearch bulk/PIT results require request
correlation; allowing an arbitrary `R` must not bypass those contracts. Streaming remains
`-> stream T` / `-> stream` with its current finite/unread distinction; generic outputs do not
supply a missing streaming protocol. Generic input bodies and per-call runtime formats are
outside this proposal.

**Simple control (Nager):** Unchanged; the macro adds generics only where the author writes them.

**Charter check:**

- C1: Schema variability is a fact of SOQL results, expressed with Rust type parameters.
- C2: The declaration retains envelope and format; traversal and validation remain Rust.
- C3: Nongeneric endpoint declarations and their generated API keep the current shape.
- C4: `::<R>` visibly selects the record decoder; no response-driven guessing.
- C5: Missing decode bounds point to the authored `R`/bound, not a generated private decoder.
- C6: “Choose the returned record type” is the caller rule.
- C7: Typed `Prepared<QueryResponse<R>>`, `into_parts` and native sends remain intact.
- C8: Exactly the same request bytes for the same inputs, independent of the selected record type.
- C9: Require Account/Contact/custom-record wire equivalence; generic `Endpoint`/`Sends` caller tests; cfg/name-collision tests; non-Clone R clone tests; and malformed/mismatched response tests retaining operation metadata.

**New concepts:** Endpoint type parameters and bounds; generic generated endpoint types.
Existing Rust traits describe decoding; no SDK registration table.

**Breaking changes:** Adopting this form replaces specialized endpoint names and operation enum
variants with generic operation names. Public request type aliases can preserve some paths;
deprecated Rust forwarding methods can preserve calls temporarily. Type inference may require
a turbofish when the result is unused.

**Why it might be wrong:** Static endpoint outputs are a valuable simplification, and this
changes macro bounds, generated type naming and documentation together. The Salesforce evidence
is concrete, but the feature should fail its trial if it requires a second generic dispatch
framework or degrades ordinary diagnostics.

## Selection and falsification

Start with **C1** for deployment-selected SDKs and **G1** for methods whose entire body is a
delegate. Trial **C2** against QuickBooks before generalizing it: the winning result is a
correct environment/realm/auth snapshot directly from `prepare()`, not just fewer setter names.
Use **G2** where a domain input is reused or currently bypassable. **G3** has a specific
Salesforce payoff but a larger compiler surface. **C3** is the alternative when checking final
requests is more valuable than structural exclusion; its low immediate line savings are explicit.

Do not add endpoint aliases, re-export directives, constructor visibility controls, fixture modes,
or policy-default expressions merely to make every remaining wrapper disappear. Alias/export
layout describes the Rust package, not the provider. In particular, permitting initialized send
policy values is a plausible standalone fix for Zendesk's F9, but
`SendPolicy::default()` is SDK behavior; it should not be smuggled into a provider-fact design as
an unexplained extra keyword.

The source currently confirms why this is open: `crates/concord-macros/src/syntax.rs:64-67`
stores only a literal client base; `crates/concord-macros/src/emit.rs:1882-1910` constructs that base and unconditionally
generates rebasing; client/scope controls are generated by `emit_owner`.
`crates/concord-macros/src/model.rs:581-589` rejects optional/default send-policy values.
Existing formats, placement composition, redaction and escape paths must remain shared machinery.

Before accepting any implementation, run a focused trial with the existing FM1–FM4 forbidden
moves, QuickBooks realm/environment replacement, S3/Kubernetes credential rotation,
ClickHouse/Kubernetes defaults, an integration-test fixture, a reqwest forward proxy and an
explicit reverse proxy. Add independent prepared/wire/canary expectations, not tests copied
from the emitter. Verify unchanged control declarations and construction calls. Retain negative
cases for redirects, because a declaration-level origin fact does not control an arbitrary
reqwest redirect policy.

**Status:** designs and source-based estimates only. No repository edits, prototype builds,
provider calls or test runs were performed. The C9 entries specify the tests a future
implementation must pass; they are not claims that the proposed guarantees have been tested.

