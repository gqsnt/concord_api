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
