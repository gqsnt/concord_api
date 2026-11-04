# V4.3 candidate — decisions

Each entry states the decision, the evidence it rests on, and what it costs. Entries marked
*pre-implementation* were written before the code they govern and are kept as written; the
outcome is recorded separately below them.

## G1 — Generated-input representation (ADR, pre-implementation)

Question: keep `api!` endpoint generation, and let the macro also emit an editable input value
per endpoint, so that applications can store, branch, inspect and rebind request inputs.

### Chosen first surface: the endpoint's own inputs, as a generated record

For every endpoint the macro emits `{EndpointType}Input` (for `Catalog.tenant.listing.list`:
`CatalogTenantListingListInput`) holding exactly the endpoint's **own** declared values and its
body, in their current state, as public fields:

| Declaration | Field |
|---|---|
| `x: T` (required) | `pub x: T` |
| `x: T = e` (initialized) | `pub x: T`, starting as `e` |
| `x?: T` (optional) | `pub x: Option<T>`, starting `None` |
| `x?: T = e` | `pub x: Option<T>`, starting `Some(e)` |
| `body b: T` / `body b?: T` | `pub b: T` / `pub b: Option<T>` |

The generated request type (`CatalogTenantListingList`) becomes *context handle + input*:

```rust
let listing = api.tenant("acme").listing().set_default_tags(vec!["featured".into()]);

// Store and branch inputs without any client context.
let mut lamps = CatalogTenantListingListInput::new(); // initializers run here, once
lamps.q = Some("lamp".into());
let mut unfiltered = lamps.clone();
unfiltered.tags = Some(Vec::new()); // explicit empty replacement of the inherited tags

// Bind to a context: the same input against two tenants.
let acme = listing.list_with(lamps.clone()).send().await?;
let studio = api.tenant("studio").listing().list_with(lamps).send().await?;

// Inspect or edit an existing request, without preparing it.
let mut request = listing.list().set_limit(5);
assert_eq!(request.input().limit, 5);
request.input_mut().cursor = Some(next);
let input = request.into_input(); // the context is dropped; rebind with `list_with`
```

It is **not** a borrowed view (that alone cannot be stored or branched), **not** a resolved wire
view, and it **copies no inherited context**: `tags: None` means "inherit what the context
supplies", exactly as `maybe_tags(None)` does today.

### Answers to the seven questions

1. **Contents.** Local state of the endpoint's own values and body: required values as supplied,
   initialized values as evaluated when the input was created, optional presence as `Option`.
   One defined combination; nothing resolved, nothing inherited.
2. **Context outside the record.** Everything owned by the client and scopes stays in their
   handles: the `reqwest::Client`, base URL, client values (`key`, `region`), scope values
   (`tenant`, `default_tags`), static header/query blocks and formats. Rebinding is
   `parent.{endpoint}_with(input)`, which snapshots the parent handle exactly as
   `parent.{endpoint}(..)` does (an `Arc` clone). An input has no shared state of its own; its
   `clone()` clones the owned field values — deep for owned data, shallow for `Arc`/interior
   mutability inside the caller's own types.
3. **Absent / inherit / empty / replacement** (the SDK's `list`):
   `tags: None` inherits the listing's `default_tags`; `Some(vec![])` sends no `tag`;
   `Some(v)` sends exactly `v`. `region: None` inherits the client's `x-region`; `Some(None)`
   sends no `x-region`; `Some(Some(r))` sends `r`. `q: None` sends no `q`. `limit` is an
   initialized scalar: always present, **omitting it is inexpressible** (the declaration has no
   absent state; the record must not invent one). Also inexpressible per call: removing a client
   or static header (`x-client`, `x-api-key`) or a scope's value other than through a declared
   override.
4. **Evaluation timing.** Initializers run when an input is created — `Input::new(..)` or the
   navigation method, which becomes `parent.list_with(Input::new(..))`, so each creation still
   evaluates each initializer once, as in V4.2. Inspection (`input()`, field reads, `Debug`),
   cloning, `_with`, setters and `into_input` never run initializers. Formatting, encoding and
   validation happen only in `prepare()`, unchanged. No resolved view is added; `prepare()` and
   `Prepared`'s redacted `Debug` remain the resolved stage.
5. **Ownership/Clone.** The input owns its values. It is `Clone` exactly when every value and
   body type is (the request type's conditional-`Clone` technique, rust-lang/rust#48214); no
   `Default`, `Serialize`, `Send` or other new bound. A non-`Clone`, move-only body is an
   ordinary field: `input()` borrows it without encoding or reading it; `into_input()` and
   `_with` move it once. A clone is a shallow structural snapshot, not deep immutability.
6. **Names and evolution.** Generated names: `{EndpointType}Input` and `{endpoint}_with` on the
   parent handle; both enter the macro's existing collision checks (type names; methods per
   generated type). The input is `#[non_exhaustive]`: downstream crates create it only with
   `Input::new(required values..)` (positional, in the navigation method's order) or from a
   request (`into_input`), then assign fields. Adding an optional or initialized value adds a
   field and keeps `new`'s signature, so a saved downstream consumer compiles unchanged. Struct
   literals, functional-update syntax and exhaustive patterns are disallowed downstream to give
   that promise. Adding a required value changes `new`, exactly as it already changes the
   navigation method (breaking either way). Inside the declaring crate Rust still permits
   literals; the promise is for downstream crates.
7. **Debug and provenance.** The input's `Debug` renders like the request type's (values
   through `Display` or their header value; sensitive values `<redacted>`; a body as `..`).
   Provenance is **static declaration provenance only**: field documentation states the starting
   state, the placement, what absence means and which enclosing owner declares the inherited
   key. Nothing records who assigned a field or when; direct field assignment is untracked, and
   the documentation says so.

### Constraints carried over

- Existing declarations, navigation methods, setters, request types, `prepare`/`send` keep their
  signatures, wire output, ordering, error stages and `Debug` text; this is additive.
- Same runtime, transport, decoders and metadata policy for both compared surfaces.
- The experiment lives on branch `concord-v4.3-dx-generated-inputs`; it is not merged into the
  supported branch unless G1-outcome adopts it. No feature flag hides it.

### Adoption criteria (fixed before the evaluation)

Adopt only if all hold: the store/branch/edit/rebind/default-understanding tasks show a concrete
caller-visible benefit in actual code and wire observations; every existing consumer, test and
example passes unchanged; the unchanged saved downstream consumer compiles after an optional
input is added; ownership, sensitive `Debug`, cfg, renamed-facade, hygiene and conditional-`Clone`
controls pass; and the added public surface and generated-code cost are proportionate. Otherwise
the prototype is retained as an evaluated experiment and excluded from the supported surface.

## S1 — Request streams and media types: guidance corrected, example added (maintained)

`docs/CUSTOM_FORMATS.md` §1 and `docs/AUTHOR_GUIDE.md` §5.7 called a streamed request body "not a
format". That contradicted `Encode`'s own contract (payload by value, any `reqwest::Body`). Both
passages now separate three cases: a fixed-media request stream *is* an `Encode<T>` (moving a
non-`Clone` source into `Body::wrap_stream`, reqwest's `stream` feature enabled by the
application); a body-dependent media type is not expressible by `Encode` alone (static
`CONTENT_TYPE`, no per-body headers) and uses the native multipart route or an explicit matching
`content-type` header that the endpoint/SDK controls; an incrementally read response uses
`send_raw`. The supplied patch was applied and extended with links to the new example.

Evidence: `examples/native/src/upload.rs` and `tests/upload.rs` — zero polls/bytes through
building, `Debug`, `prepare()` and inspection; one `send()` delivers the exact bytes chunked with
the fixed media type; replay needs a fresh source (`compile_fail` doc tests show the request and
`Prepared` are not `Clone`); a declared media type or a `content-type` header replaces the static
one. reqwest's `stream` feature is declared by `examples/native` only (`cargo tree -i reqwest -p
concord` shows no `stream`). No core change.

## D1/W1 — Domain adapters, named workflows, bounded traversal (maintained, application-owned)

`examples/app`: synchronous, fallible, service-specific conversions from the application's
`AccountId`/`Moment` that return the **generated operation** (`CatalogActivity`,
`LedgerEntries`), so callers keep setters, `prepare` and `send`, and representation errors are
typed and raised before any request. Each service owns its wire form (Catalog: decimal id,
whole-second RFC 3339; Ledger: `acct-` hex, exact nonnegative milliseconds). `report.rs` attempts
each service independently (no aggregate `?`); this aggregation is application policy, not a core
rule. `Browser::{featured_page, unfiltered_page}` names a real application intent and returns the
ordinary operation; nothing mirrors the whole generated API. `traversal.rs` is one shared,
bounded loop with per-service adapters: it returns a continuation when the bound is reached,
accepts one to resume, and on a failed page returns the records collected so far with the
continuation that refetches the failed page (keep or discard is the caller's choice; nothing is
retried). No pagination, retry or representation trait was added to the core: none was needed.

## M1 — Metadata with the typed value: maintained recipe; public convenience rejected

`examples/native/src/metadata.rs` is the corrected recipe: prepared → `into_parts` → the caller's
native edit → exactly one `execute` → selected head metadata copied before the body is read → the
endpoint's own `Decoder`. A failure before any head is converted by `decoder.finish(Err(e))`
(Concord's documented transport conversion: no URL), never returned raw. `MetaError` carries the
`concord::Error` and `Option<Meta>`: `None` when no head arrived; `Some` for `Status`/`Metadata`
(the unread response stays inside the Concord error), body-read `Transport` and `Decode` (only the
captured head survives; the body is gone). `Debug`/`Display` of `Meta`, `WithMeta`, `Received` and
`MetaError` are written by hand: status, version and kept header *names* only; values, the URL
and the decoded value only through explicitly named `raw_*` accessors and the public field.

It is not the archived `Pending` helper: that one executed internally (no edit point), returned a
raw `reqwest::Error` (the M02 control test shows that route printing the synthetic secret URL
while the recipe's does not) and derived `Debug` over a full `HeaderMap`.

**Public convenience: rejected.** Everything the recipe needs is public (`into_parts`,
`Decoder::{decode, finish}`, `Error::{response, into_response, transport}`). The only awkward line
is `let Err(error) = decoder.finish(Err(e)).await else { unreachable!() }` (also present in the
archived A099 check). A `Decoder::transport_error` would save those three lines once per helper
but adds a second route for something documented and working; a core `send_with_meta`/`Reply<T>`
would fix which headers survive, the `Debug` policy and the error wrapper — application policy the
core deliberately does not own (D03). The caller guide now points at `finish(Err(e))`.

## X1 — Outside operations: SDK-owned context (maintained); core primitive rejected

`examples/sdk/src/context.rs` is the public boundary, frozen (commit `cf3c9fc`) before the
consumer was written and not changed afterwards: one `CatalogContext` (client, base, key, region)
yields both the generated handles and native requests for undeclared routes, so they cannot
drift; `TenantContext::request(method, segments)` keeps the base prefix, encodes each segment as
Concord encodes a path value, refuses empty/`.`/`..` before I/O, and adds `x-client`, the key with
the sensitive flag and the region. `examples/extension` (depending only on `concord` and the SDK)
adds `GET tenants/{tenant}/exports/{export}` — not a suffix of any declared route — with its own
`Csv` format and `Decoder::new`, so its errors are truthfully unlabeled (`endpoint()` is `None`)
and its wrapper names the extension operation; no seed endpoint, no `__private`.

**Core/generated primitive: rejected for now.** All X01–X05 checks pass with no change under
`crates/`. The SDK duplicates about 35 lines (segment encoding, dot-segment refusal, base joining,
header composition), guarded by agreement tests that compare 1,450 context-built requests with the
generated endpoints' output and fail on drift (a mutation of the encoder fails two of them). A
generated per-scope native-request builder is the only option that would remove the duplication,
but it adds surface on every scope and has open questions (inherited query/header chains,
build-error identity without an endpoint, redaction of path data it cannot classify); exposing
`Prepared::new` or reassembly with a stale decoder would re-attach generated identity and
redaction metadata to requests it no longer describes. Revisit if several SDKs need such contexts.
