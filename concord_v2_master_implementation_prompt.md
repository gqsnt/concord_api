# Concord API v2 — Master Implementation Prompt

This document is a standalone implementation prompt. Give it to a coding agent together with the Concord repository. The prompt is intentionally normative: it selects one coherent v2 rather than asking the implementer to reopen the entire design space.

---

# BEGIN IMPLEMENTATION PROMPT

## Role and operating mode

You are the lead Rust language, procedural-macro, API-design, and runtime engineer responsible for transforming the Concord API repository from its current v1 language into Concord v2.

Work directly in the repository. Do not stop after analysis, an architecture plan, parser changes, or isolated prototypes. Implement the complete vertical system:

```text
v2 declaration
→ parsed syntax
→ canonical resolved semantic model
→ generated Rust API
→ planning and contract reflection
→ managed execution
→ errors and operational evidence
→ documentation, examples, migration guidance, and tests
```

The task is complete only when the repository expresses one coherent v2 across all of those projections and the full verification gate passes.

Do not preserve weak v1 behavior merely because it is already public. This is a deliberate major-version redesign. Do not maintain parallel v1 and v2 languages, compatibility features, or two executor paths. Old syntax may be recognized only to emit targeted migration diagnostics; it must not remain a second supported semantic mode.

Do not ask for design clarification when this prompt states a rule. Where an implementation detail is genuinely unspecified, choose the smallest design that makes the stated properties true, record it in the v2 design/migration documentation, and keep the decision local and reversible.

## Sources of truth and precedence

Use these sources in the following order:

1. **This prompt** is the normative v2 contract.
2. Existing design invariants remain authoritative where this prompt does not deliberately replace them.
3. `concord_dsl_usage_language_design_study_v2.md` supplies rationale, edge cases, and additional examples.
4. Existing code, tests, and documentation are evidence of current behavior, not a reason to preserve v1 syntax or accidental semantics.

When sources disagree, follow the highest-precedence source. Never silently combine incompatible v1 and v2 meanings.

## Mission

Concord v2 must be:

> **A contract language that projects one semantic API model into a domain-shaped Rust client, a bounded managed execution, and inspectable operational evidence.**

The macro and core exist to support that product. They are not the primary user experience. Judge every design by what an API author, client creator, endpoint caller, reviewer, operator, and extension author must read and can correctly predict.

A declaration is successful only when a reader can predict, without knowing macro internals:

- the domain input and output types;
- which values are required, optional, or defaulted;
- who owns each value;
- which constructor or control method is generated;
- how values affect host, path, query, headers, body, authentication, rate keys, pagination, and results;
- which terminal methods exist;
- when effects begin;
- what may retry, wait, paginate, recover, or time out;
- what appears in plans, errors, hooks, descriptions, and logs;
- which runtime component owns each decision.

## Product non-goals

Do not turn Concord into any of the following:

- a general native `reqwest` mutation layer;
- a universal middleware framework;
- a complete OpenAPI replacement;
- a full algebra for every HTTP status/content-negotiation possibility;
- a general Rust expression evaluator inside the DSL;
- a typestate experiment whose generic state leaks throughout generated APIs;
- a dependency-injection container or universal context object;
- a policy language that moves application retry, transport, proxy, or operational policy into endpoint declarations;
- a compatibility shell carrying both v1 and v2 indefinitely.

Add a mechanism only when it earns a named property required by this prompt. Prefer a concrete enum, generated structure, or narrow trait over a generic framework.

## Quality order

When two choices conflict, optimize in this order:

1. semantic truthfulness;
2. security, authority control, and finite resource behavior;
3. local comprehensibility and predictable generated usage;
4. domain-shaped caller ergonomics;
5. actionable errors and diagnostics;
6. coherent reflection and operational evidence;
7. narrow, meaningful extensibility;
8. performance and compile-time cost;
9. syntactic brevity.

Brevity must never depend on hidden type-name recognition, hidden merge behavior, unrestricted callbacks, or stronger-looking names than the runtime guarantee.

## Non-negotiable system invariants

Preserve or establish all of the following:

1. **One execution authority.** Generated direct requests, unbound endpoints after binding, runtime-defined endpoints, endpoint-backed credential operations, retry attempts, authentication recovery, pagination pages, response terminals, and byte terminals all use one managed executor.
2. **Syntax-neutral core.** The macro resolves syntax into semantic data and generated Rust. `concord-core` does not depend on parser syntax or recognize DSL spellings.
3. **Public before private.** Public request shaping and credential-free observation occur before private authentication placement. Credentials never enter public expressions, public plans, public hooks, rate keys, descriptions, or errors.
4. **Immutable snapshots.** Built clients, scopes, endpoint intents, and requests are immutable values. Controls consume one value and return another.
5. **Description before execution.** Request construction, endpoint construction, descriptions, and plans are inert. Network and other managed effects begin only when an execution terminal is polled/awaited.
6. **Finite behavior.** Every growing dimension—formatted atoms, URL parts, headers, request and response bytes, records, streams, retries, pages, items, cycle state, provider traffic, rate state, diagnostic data, and observer queues—has a finite bound and a first-excess test.
7. **Capability-shaped API.** A generated method exists only when the endpoint declares and the core can enforce that capability.
8. **Narrow extension authority.** Extensions vary one semantic decision and cannot bypass route ownership, authentication isolation, retry gates, deadlines, rate admission, body limits, response limits, or execution ordering.
9. **Stable identity.** Endpoint, rate-policy, operation, attempt, and page identity are distinct. Source names do not silently become durable identity when an explicit ID is required.
10. **One semantic graph.** Generated code, Rustdoc, contract descriptions, diagnostics, planning metadata, collision checks, and semantic diffing derive from the same resolved model.
11. **No incidental type language.** Rust type tokens are opaque. No syntax path such as `Option`, `Vec`, an alias, or a qualified path changes Concord semantics.
12. **Runtime truth.** A fluent method name and DSL clause must mean what the complete runtime actually guarantees, including pagination, retries, recovery, cancellation, and response acquisition.

## Repository baseline

Before changing code, inspect the repository and preserve its useful architecture:

- workspace crates: `concord-macros`, `concord-core`, `concord`, `concord-extras`, examples, test support, `xtask`, and performance tools;
- Rust edition 2024;
- MSRV 1.86;
- workspace-wide `unsafe_code = "forbid"`;
- the current hidden macro/core generated-contract handshake, presently version 31;
- existing bounded writers, request preparation, response acquisition, credential slots, provider isolation, limiter contracts, pagination controls, errors, hooks, UI tests, fixtures, and verification tooling.

Do not rewrite working core mechanisms merely to match a new naming scheme. Refactor where the semantic model or runtime guarantee changes; otherwise adapt existing mechanisms through the new canonical IR and bridge.

At completion:

- set workspace version to `2.0.0`;
- keep edition 2024 and MSRV 1.86 unless an unavoidable compiler limitation is demonstrated and documented;
- bump the generated contract handshake from 31 to 32 in macro and core together;
- remove or rewrite documents that claim v1 is frozen;
- leave no production-facing v1 syntax or aliases.

---

# 1. Canonical v2 language

The following example is the target style and must compile after the migration. Names may differ only where the repository’s existing public types require an equivalent exact spelling; the semantic model may not differ.

```rust
api! {
    /// Typed client for the catalog service.
    client CatalogApi {
        base "https://catalog.example.test/root/";
        timeout 20s;

        values {
            /// Tenant whose catalog is visible.
            #[redact]
            tenant: TenantId;

            /// Optional localization preference.
            locale?: Locale;

            /// API version segment selected for this client snapshot.
            #[default]
            api_version: ApiVersion;
        }

        credentials {
            secret bootstrap_key;
            credential application_key = api_key(secret.bootstrap_key);
            credential session = endpoint authenticate;
        }

        subdomain ["api"];
        path [client.api_version];

        query {
            "tenant" = client.tenant;
            "locale" = client.locale;
        }

        headers {
            "X-Client-Version" = "2";
        }

        auth {
            header "X-Bootstrap-Key" = application_key;
        }

        rate_limits {
            tenant_reads(tenant) {
                id "catalog.tenant.read";
                key [host, tenant];
                cost 1;
                allow 120 per 1m;
                allow 5_000 per 1h;
            }
        }

        /// Creates a session credential.
        POST authenticate(
            body input: LoginInput using Json<LoginInput>,
        ) -> SessionToken using Json<SessionToken> {
            id "catalog.auth.login";
            path ["auth", "session"];
            auth replace {
                header "X-Bootstrap-Key" = application_key;
            }
            repeatability never;
        }

        scope regional(
            region: Region,
            language?: Language,
        ) {
            subdomain [regional.region];
            path ["regions", regional.region];

            headers {
                "Accept-Language" = regional.language;
            }

            auth replace {
                bearer session recover on unauthorized;
            }

            rate_limit tenant_reads(tenant = client.tenant);

            /// Returns one page of item summaries.
            GET item_summaries(
                category?: Category,
                offset: u64 = 0,
                count: PageSize = DEFAULT_PAGE_SIZE,
                tags: Vec<Tag>,
            ) -> Vec<ItemSummary> using Json<Vec<ItemSummary>> {
                id "catalog.items.list";
                path ["items"];
                query {
                    "category" = endpoint.category;
                    "offset" = endpoint.offset;
                    "count" = endpoint.count;
                    "tag" = each endpoint.tags;
                }
                paginate with OffsetPagination {
                    offset -> endpoint.offset;
                    page_size -> endpoint.count;
                }
            }

            /// Returns one item.
            GET item(id: ItemId) -> Item using Json<Item> {
                id "catalog.item.get";
                path ["items", endpoint.id];
                expose accepted_bytes;
            }

            /// Applies an idempotent update identified by `command_id`.
            PUT update_item(
                id: ItemId,
                #[redact]
                command_id: CommandId,
                body update: UpdateInput using Json<UpdateInput>,
            ) -> Item using Json<Item> {
                id "catalog.item.update";
                path ["items", endpoint.id];
                headers {
                    "Idempotency-Key" = endpoint.command_id;
                }
                repeatability idempotent;
                rate_limit {
                    id "catalog.item.write";
                    key [host, endpoint, client.tenant];
                    cost 5;
                    allow 30 per 1m;
                }
            }
        }
    }
}
```

Representative generated usage must look like this:

```rust
let client = CatalogApi::builder()
    .set_tenant(tenant)
    .with_locale(Locale::FrFr)
    .reset_api_version()
    .bootstrap_key(SecretString::try_new(raw_key)?)
    .retry(retry_policy)
    .configure_http(|http| {
        http.require_https()
            .connect_timeout(Duration::from_secs(3))
    })
    .configure_runtime(|runtime| {
        runtime
            .response_body_limit(nonzero!(8 * 1024 * 1024))
            .pagination_max_pages(nonzero!(100))
            .rate_limiter(limiter)
    })
    .build()?;

client.authenticate(login).store_as_session().await?;

let eu = client
    .regional(Region::EuWest)
    .with_language(Language::French);

let request = eu
    .item_summaries(tags)
    .set_category(category_from_config) // Option<Category>
    .reset_count();

let plan = request.plan()?;
let items = request
    .paginate(PageLimit::CompleteWithin(nonzero!(20)))
    .await?;

let response: ApiResponse<Bytes> = eu
    .item(item_id)
    .accepted_bytes()
    .await?;
```

Unbound construction must preserve the same domain tree:

```rust
let endpoint = CatalogApi::unbound()
    .regional(Region::EuWest)
    .item(item_id);

let description = endpoint.description();
let plan = client.plan(&endpoint)?;
let request = client.request(endpoint);
let item = request.await?;
```

The direct and unbound paths must resolve to the same endpoint identity, values, route, attachments, capabilities, and execution behavior once bound to the same client.

---

# 2. Grammar and ownership

## 2.1 High-level grammar

Implement this conceptual grammar:

```text
api              = "api!" "{" client "}"
client           = attributes "client" TypeIdent "{" client_item* "}"

client_item      = base
                 | timeout
                 | values_block
                 | credentials_block
                 | rate_limits_block
                 | attachment
                 | scope
                 | endpoint

scope            = attributes "scope" ident "(" parameters? ")"
                   "{" scope_item* "}"
scope_item       = attachment | scope | endpoint

endpoint         = attributes method ident "(" parameters? ")"
                   "->" Type "using" Type
                   (";" | "{" endpoint_item* "}")

parameter        = attributes value_parameter | body_parameter
value_parameter  = ident ["?"] ":" Type ["=" stable_value]
body_parameter   = "body" ident ":" Type "using" Type

attachment       = subdomain | path | query | headers | auth | timeout
                 | rate_limit
endpoint_item    = endpoint_id | attachment | accept | repeatability
                 | pagination | expose
```

The parser must accept arbitrary valid Rust type syntax in every `Type` position and preserve it as opaque tokens. It must not inspect a type path to decide optionality, cardinality, body behavior, response behavior, or rate-key behavior.

## 2.2 Lexical containment

All root scopes and endpoints live inside their owning `client` block. Nested scopes and endpoint leaves live inside their lexical parent.

The following trees must be isomorphic:

- declaration tree;
- generated direct navigation tree;
- generated module/type tree;
- unbound endpoint tree;
- contract-description ownership tree.

One `api!` invocation contains exactly one client. Multiple clients use multiple invocations.

No source-level child may appear after the client block as in v1. Recognize that form only to emit a migration diagnostic explaining that children now belong inside the client.

## 2.3 Owner namespaces

Replace generic and misleading references with owner-qualified references:

```rust
client.tenant
regional.region
endpoint.item_id
```

Rules:

- `client.NAME` resolves a client-owned value.
- `SCOPE_IDENT.NAME` resolves a value owned by that exact visible scope.
- `endpoint.NAME` resolves an endpoint-owned ordinary value.
- body inputs are not public-expression values.
- secrets are referenced only as `secret.NAME` inside credential-source declarations.
- credentials are referenced by their declared credential name only inside authentication declarations.

Remove `vars.*`, generic `scope.*`, and `ep.*` from the supported language.

Reject duplicate scope identifiers in one ancestor chain. A scope identifier must uniquely identify one visible owner from any descendant attachment site. Sibling branches may reuse a scope identifier because they are never simultaneously visible.

Resolution must bind every reference to a stable internal `OwnerId` and `ValueId`; emitters must not repeat textual lookup.

## 2.4 Documentation and supported attributes

Capture `///` documentation as contract data and project it into generated Rustdoc and contract descriptions.

Support these attributes only:

- `#[default]` on ordinary values;
- `#[redact]` on ordinary public values;
- `#[deprecated(note = "...")]` on clients, scopes, and endpoints;
- Rust doc attributes produced by `///`.

Do not implement `#[default_with]` in the initial v2. Do not pass arbitrary attributes through the macro. Emit a targeted error for unsupported attributes.

A value-level deprecation attribute is not required in v2 because required positional parameters cannot be deprecated coherently. Do not implement a partial value-deprecation projection.

---

# 3. Values, presence, defaults, and generated controls

## 3.1 Exact type law

This is the central v2 rule:

> **`?` adds exactly one Concord-owned outer presence layer. The written Rust type otherwise means exactly itself.**

Implement this matrix exactly:

| Declaration | Payload type | Effective stored type | Required constructor/build input? |
|---|---|---|---|
| `x: T` | `T` | `T` | yes, unless defaulted |
| `x?: T` | `T` | `Option<T>` | no |
| `x: Option<T>` | `Option<T>` | `Option<T>` | yes, unless defaulted |
| `x?: Option<T>` | `Option<T>` | `Option<Option<T>>` | no |
| `x: Vec<T>` | `Vec<T>` | `Vec<T>` | yes, unless defaulted |
| `x?: Vec<T>` | `Vec<T>` | `Option<Vec<T>>` | no |

Aliases and qualified paths do not change the matrix:

```rust
type MaybeOwner = Option<UserId>;

a: MaybeOwner,   // required payload
b?: MaybeOwner,  // Option<MaybeOwner>
```

Delete semantic variants such as `ValueShape::Option`, `ValueShape::Vec`, and `ValueShape::OptionVec` if they encode token recognition. Replace them with independent semantic facts such as:

```text
payload_type: opaque Rust Type
presence: Required | Optional
initializer: None | TraitDefault | StableValue
observation: Ordinary | Redacted
owner: OwnerId
```

The macro may retain helper information needed to generate an outer `Option`, but it must never unwrap or classify the payload type.

## 3.2 Meaning of required `Option<T>` and nested optionality

Preserve these meanings:

```rust
new_owner: Option<UserId>
```

The caller must provide a domain decision; `None` is a valid payload.

```rust
new_owner?: Option<UserId>
```

The effective type is `Option<Option<UserId>>`:

```text
None             = omit the decision
Some(None)       = explicitly clear the remote value
Some(Some(user)) = explicitly set it
```

Do not flatten these states in storage, setters, reflection, query omission, or diagnostics.

## 3.3 Defaults

Replace v1 `= default` with:

```rust
#[default]
x: T
```

`#[default]` means `<T as Default>::default()` where `T` is the payload type. Apply the default first, then the Concord presence layer:

```rust
#[default]
x: T             // T::default()

#[default]
x?: T            // initial Some(T::default())

#[default]
x: Option<T>     // initial payload None

#[default]
x?: Option<T>    // initial Some(None)
```

Keep explicit stable defaults:

```rust
x: T = CONST_VALUE
x?: T = CONST_VALUE // initial Some(CONST_VALUE)
```

`#[default]` and `= VALUE` are mutually exclusive. Emit a direct error when both appear.

The accepted explicit initializer language is deliberately narrow:

- string, char, boolean, and signed numeric literals;
- simple const/static paths already supported by the repository;
- no calls, methods, macros, blocks, closures, indexing, mutable expressions, collection construction, or arbitrary Rust expressions.

Do not add `#[default_with]` until a real use case proves that a stable const/path or domain `Default` is insufficient.

## 3.4 Default evaluation time

Evaluate each declaration default exactly once at its narrowest owning snapshot:

| Owner | Evaluation boundary |
|---|---|
| client value | each successful client build when no explicit override is selected |
| scope value | creation of that scope snapshot |
| endpoint value | construction of that endpoint intent/request snapshot |

Retries, authentication recovery, and pagination pages reuse the retained value. They must not reevaluate a default.

A reset restores the retained evaluated default without calling `Default` or reevaluating the explicit expression. Preserve this without requiring payload `Clone`; use the existing `Arc`-based retained-value strategy or an equivalent ownership-safe design.

For a client builder, `reset_x()` removes the builder override. The next `build()` evaluates the declaration default once. For an already-created scope or request, `reset_x()` restores the retained default value.

## 3.5 Generated controls

Generated controls consume `self` and return the new immutable snapshot.

### Required, non-defaulted client value

```rust
fn set_x(self, value: T) -> Self;
```

`build()` fails with a deterministic `ClientBuildError` if it was never supplied.

### Required scope or endpoint value

It is a positional constructor argument in declaration order. Do not generate a public setter for a required non-defaulted scope or endpoint value. To change it, construct a new scope/endpoint intent. Internal pagination bindings may update private generated slots through a sealed adapter.

### Optional value

For `x?: T`, generate:

```rust
fn set_x(self, value: Option<T>) -> Self;
fn with_x(self, value: T) -> Self;
fn clear_x(self) -> Self;
```

Laws:

```text
set_x(Some(v)) == with_x(v)
set_x(None)    == clear_x()
```

For `x?: Option<U>`, the exact signatures become:

```rust
fn set_x(self, value: Option<Option<U>>) -> Self;
fn with_x(self, value: Option<U>) -> Self;
fn clear_x(self) -> Self;
```

Do not use `impl Into<Option<T>>`. Exact types are required for predictable nested optionality and inference.

### Defaulted value

A defaulted value also receives:

```rust
fn reset_x(self) -> Self;
```

For a required defaulted value, generate `set_x(T)` and `reset_x()`.

For an optional defaulted value, generate `set_x(Option<T>)`, `with_x(T)`, `clear_x()`, and `reset_x()`.

`clear` and `reset` must remain observably different when the retained default is present.

### Builder state

Represent builder overrides with a named internal state, not ad hoc nested option logic. Conceptually:

```rust
enum Override<T> {
    UseDeclaration,
    Set(T),
}
```

`T` may itself be `Option<Payload>` for a Concord-optional value.

Do not retain an “explicitly absent” provenance bit when it has no behavioral effect. Preserve only distinctions required for construction, reset, placement, reflection, or execution.

## 3.6 Constructor arguments

Generated scope and endpoint methods include only required non-defaulted ordinary values and body inputs. Preserve their source declaration order after omitting optional/defaulted ordinary values.

Examples:

```rust
scope regional(region: Region, language?: Language)
```

generates:

```rust
client.regional(region)
```

and the returned scope has `set_language`, `with_language`, and `clear_language`.

```rust
POST update(
    id: ItemId,
    mode?: Mode,
    body input: UpdateInput using Json<UpdateInput>,
    trace_id: TraceId,
)
```

generates a direct method whose required arguments are `(id, input, trace_id)` in that order.

Do not create adaptive parameter structs based on argument count. Authors who have too many required values should introduce a meaningful domain input type.

## 3.7 Generated-name collision checks

Resolve and diagnose collisions for every generated symbol, including:

- client, builder, support module, scope, endpoint, intent, and request type names;
- scope and endpoint methods;
- `set_*`, `with_*`, `clear_*`, and `reset_*`;
- `plan`, `description`, `response`, `accepted_bytes`, `paginate`, timeout controls, debug controls, and credential storage terminals;
- `store_as_*` methods;
- unbound-tree methods;
- internal bridge names where source identifiers can collide.

The diagnostic must identify both source declarations and the generated name. Never silently suppress a convenience method.

---

# 4. Repetition and cardinality

`Vec`, arrays, sets, aliases, qualified paths, and all other payload types are opaque. Cardinality belongs to the placement expression.

## 4.1 Query emission

One-value emission:

```rust
"filter" = endpoint.filter;
```

Repeated emission:

```rust
"tag" = each endpoint.tags;
```

`each` must operate on a direct owner reference, not an arbitrary `concat`. It iterates the payload by shared reference through standard `IntoIterator` semantics or a narrow core helper with an equivalent blanket implementation. The macro must not recognize a concrete collection type.

For `items?: T`:

- outer `None` emits no entries;
- outer `Some(payload)` applies one-value or `each` behavior to the payload.

For a required payload `Option<U>`, Concord does not treat it as a presence slot. It participates only through the traits required by the chosen placement. For example, `each endpoint.required_option` may work through `IntoIterator for &Option<U>`; ordinary scalar formatting requires the payload itself to implement the relevant formatting contract.

Iteration order is contract-visible. It affects query order, request identity, pagination cycle identity, and deterministic tests.

## 4.2 No initial repeated-header language

Do not add `each`, `+=`, or generic repeated-value behavior to public headers in the first v2 implementation. HTTP field-combination rules vary by field. A caller needing one encoded header value should use a domain type whose `Display` implementation owns that representation.

Keep header repetition as a deferred, separately designed feature.

## 4.3 Canonical rate-key values

Rate-key cardinality does not use `each` or display formatting. It uses the dedicated canonical rate-key contract defined later in this prompt.

---

# 5. Restricted public expression language

## 5.1 Rename `fmt` to `concat`

Replace:

```rust
fmt["item-", endpoint.id]
```

with:

```rust
concat ["item-", endpoint.id]
```

The name must state the operation actually performed.

## 5.2 Expression grammar

Support only:

```text
public_atom = string_literal
            | supported integer literal
            | owner_ref
            | concat

owner_ref   = client.ident
            | visible_scope_ident.ident
            | endpoint.ident

concat      = "concat" "[" public_atom ("," public_atom)* ","? "]"
```

Flatten nested concatenations into one bounded write plan. Do not retain unbounded intermediate strings.

Reject:

- function and method calls;
- indexing;
- field chains beyond the one owner/value separator;
- macros;
- closures;
- blocks;
- conditionals and matching;
- arithmetic;
- arbitrary Rust expressions;
- secrets or credentials.

## 5.3 Total and omission-aware contexts

An outer Concord-optional value may be used directly only in omission-aware placements such as query and header assignment.

Reject optional slots in total scalar contexts:

- subdomain labels;
- path segments;
- `concat` children;
- explicit response `accept`;
- rate-key dimensions;
- any context where “absence” would silently change route structure.

A required/defaulted payload whose Rust type happens to be `Option<T>` is not a Concord-optional slot. It may be used if that payload implements the context’s required trait.

## 5.4 Context-specific conversion

Do not introduce one universal `WireValue` trait.

Use the existing bounded `Display`-style conversion for public URL/header atoms where appropriate, followed by context-specific validation and encoding:

- subdomain: ASCII DNS-label validation;
- path: segment validation then percent encoding;
- query: query-component encoding;
- headers: `HeaderValue` validation;
- response accept override: media/header validation.

Rate keys use a separate canonical trait because human display text is not a stable coordination identity.

---

# 6. Base origin, subdomain, and path

## 6.1 Fixed base

`base` is required and client-only:

```rust
base "https://api.example.test/root/";
```

Validate at macro expansion:

- absolute `http` or `https` URL;
- host present;
- no user info;
- no query;
- no fragment;
- optional fixed base path allowed.

The base defines the fixed origin/trust boundary. Do not add a raw per-request base URL override. Transport proxy configuration remains transport policy and must not become endpoint routing.

## 6.2 `subdomain` replaces `host`

Implement:

```rust
subdomain ["api"];
subdomain replace ["legacy"];
subdomain off;
```

Semantics over generated labels:

- omission inherits the current generated-label list;
- `subdomain [local...]` prepends local labels to inherited generated labels;
- `subdomain replace [local...]` replaces all inherited generated labels;
- `subdomain off` clears generated labels and returns to the fixed base host.

Example:

```text
base host: example.test
client subdomain: api
scope subdomain: eu
result: eu.api.example.test
```

Each atom creates exactly one label. Validate static labels at expansion and dynamic labels at planning/execution. Preserve existing per-label and total-host byte bounds. Require an ASCII DNS-domain base when any effective subdomain operation exists; reject IP-literal bases for such contracts.

Do not expose a raw host/authority expression.

## 6.3 Path algebra

Implement:

```rust
path ["v1", "items"];
path replace ["health"];
path off;
```

Semantics:

- omission inherits;
- `path [...]` appends local generated segments;
- `path replace [...]` replaces inherited generated segments;
- `path off` removes all generated segments;
- the fixed path from `base` is never removed by these operations.

Each atom is exactly one URL segment. Reject pre-encoding values and reject empty, `.`, `..`, `/`, and `\` segments. Encode once after validation. Preserve existing segment/count/URL bounds and first-excess behavior.

Do not add optional path branching, matrix parameters, encoded-segment escape hatches, or trailing-slash syntax in initial v2.

---

# 7. Query as an ordered-multimap patch language

The effective query is an ordered sequence of `(key, value)` pairs. It is not a map.

## 7.1 Syntax

```rust
query {
    "tenant" = client.tenant;
    "locale" = client.locale;
    "tag" = each endpoint.tags;
    "filter" += endpoint.additional_filter;
    remove "legacy";
}

query replace {
    // starts from an empty query
}

query off;
```

Keys are static string literals, compared exactly and case-sensitively.

## 7.2 Operational semantics

Process inheritance and local statements deterministically in source order.

### Assignment

```rust
"key" = emission;
```

1. Remove every current occurrence whose key exactly equals `key`.
2. Evaluate the emission.
3. Append its zero, one, or many pairs at the position of this statement.

An absent optional emission therefore means **replace inherited values with absence**.

### Append

```rust
"key" += emission;
```

Evaluate and append zero, one, or many occurrences. Do not remove inherited or prior local entries.

An absent optional append contributes nothing and leaves inherited entries unchanged.

### Remove

```rust
remove "key";
```

Remove all current exact-key occurrences.

### Replace block

`query replace { ... }` starts evaluation from an empty ordered list.

### Off

`query off;` produces an empty query and is exclusive at its attachment site.

## 7.3 Emissions

Support:

- one bounded scalar atom;
- one omission-aware direct optional reference;
- `each` over a direct required or optional owner reference.

Allow repeated local statements for the same key. For example, an assignment followed by append is meaningful. Remove the v1 “local names are unique” rule.

Do not implement `?=` in v2. The distinction between assignment and append is sufficient until a concrete contract demonstrates a third stable operation.

Do not replace the explicit query DSL with automatic `serde` serialization of an arbitrary parameter struct. Such a codec may exist as a specialist helper later, but it must not hide key names, multiplicity, omission, ordering, or inheritance in the common language.

## 7.4 Query invariants

- Public query shaping occurs before private auth query placement.
- Public/private key collisions are detected before dispatch.
- Source order is deterministic and reflected.
- Query values are bounded before retention.
- The same pure resolved query plan is used by planning and execution.
- Credentials and secret values cannot participate.
- Redacted values use real bytes internally and redacted observation externally.

---

# 8. Public headers and media ownership

## 8.1 Header patch syntax

```rust
headers {
    "X-Tenant" = client.tenant;
    "X-Locale" = client.locale;
    remove "X-Legacy";
}

headers replace { ... }
headers off;
```

Header identity is ASCII case-insensitive.

Assignment removes every inherited/current value for that field name and inserts one value at the local statement position. If an outer Concord-optional value is absent, assignment removes inherited values and inserts nothing.

`remove` removes the effective field. `replace` starts empty. `off` clears all public headers.

Do not add a header append operator in initial v2.

## 8.2 Reserved ownership

Retain existing transport/proxy/connection/cookie/private-header restrictions and make media ownership consistent.

Public `headers` must reject at least:

- `Host`;
- `Authorization` and `Proxy-Authorization`;
- connection/upgrade/transfer fields owned by the HTTP stack;
- cookie fields owned by explicit cookie/transport policy;
- `Content-Length`;
- `Content-Type`;
- `Accept`.

The exact reserved set should build on current validated categories, not a looser replacement.

## 8.3 `Content-Type`

`RequestBodySpec` exclusively owns request media metadata, including `Content-Type`. Public headers and auth placements cannot write it.

## 8.4 `Accept`

`ResponseSpec` owns the default `Accept` value. An endpoint may explicitly override or suppress it only through response-owned syntax:

```rust
accept endpoint.media_type;
accept off;
```

`accept` is endpoint-only, scalar, total, bounded, and validated as a header/media value. It must not be inherited independently of the endpoint’s response contract.

A public header named `Accept` is always a declaration error with a diagnostic directing the author to `accept`.

---

# 9. Request body declarations

## 9.1 Syntax and signature truth

Replace v1:

```rust
#[body] payload: Json<UpdateInput>
```

with:

```rust
body payload: UpdateInput using Json<UpdateInput>
```

The first type is exactly what the caller supplies. The `using` type is the request body specification.

Generate an endpoint-specific compile-time assertion equivalent to:

```rust
Json<UpdateInput>: RequestBodySpec<Input = UpdateInput>
```

Use generated named aliases/assertion helpers so type errors point at the endpoint and explain the mismatch as clearly as stable Rust permits.

## 9.2 Body rules

- At most one body parameter.
- Endpoint-only.
- May appear anywhere in the parameter list.
- Participates in the required direct-method argument order at its source position.
- Cannot use `?`, `#[default]`, `= VALUE`, or `#[redact]`.
- Has no generated setter/resetter.
- Is never a public shaping reference.
- Is not visible to plans, rate keys, ordinary hooks, descriptions containing runtime values, errors, or debug formatting.

Optional body semantics use an explicit payload type:

```rust
body payload: Option<UpdateInput> using OptionalJson<UpdateInput>
```

The caller still supplies the argument. The body spec decides how `None` is encoded. Concord does not infer body absence from type spelling.

## 9.3 Preserve lifecycle responsibilities

Keep the strong current body architecture:

```text
caller input
→ body-spec metadata
→ preparation under core control
→ prepared body
→ per-attempt/page materialization
→ native body
```

The body specification owns typed input interpretation, media metadata, preparation, and its explicit replay/page-stability contract. Core owns timing, bounds, ordering, retry gates, page execution, and native request construction.

Mechanical replayability is not semantic endpoint repeatability. Keep them as independent properties.

A paginated body must implement the page-stable body contract; cloning alone is not proof of page stability.

---

# 10. Response declarations and terminals

## 10.1 Rename and output-first syntax

Rename public `ResponseBodySpec` to `ResponseSpec` throughout core, extras, facade exports, generated code, docs, and examples.

Declare:

```rust
-> Item using Json<Item>
```

The arrow type is exactly what `.await` returns. The `using` type is the response specification.

Generate an endpoint-specific compile-time assertion equivalent to:

```rust
Json<Item>: ResponseSpec<Output = Item>
```

Do not retain a public compatibility alias for `ResponseBodySpec`.

## 10.2 Response-spec authority

A `ResponseSpec` owns:

- accepted status semantics;
- default `Accept` metadata;
- one sealed acquisition mode;
- conversion from the accepted semantic response head and core-owned acquired representation to `Output`.

Core owns:

- authentication challenge handling before final status interpretation;
- acquisition bounds and native response polling;
- buffered/streaming/discard mechanics;
- deadline and cancellation boundaries;
- error classification;
- terminal orchestration.

Keep acquisition modes sealed and finite:

```text
Buffered
Streaming
Discard
```

Do not let downstream code define a new acquisition mode that bypasses core bounds.

## 10.3 Generated terminals

Every endpoint request supports:

```rust
request.await                    // Output
request.response().await         // ApiResponse<Output>
```

Only explicitly capable endpoints support:

```rust
request.accepted_bytes().await   // ApiResponse<Bytes>
request.paginate(limit).await
request.store_as_credential().await
```

Replace `response_bytes()` with `accepted_bytes()` and remove the old public name.

`accepted_bytes()` means:

- retain declared authentication challenge behavior;
- retain declared accepted-status behavior;
- retain response negotiation (`Accept` or explicit override);
- retain response-head and byte bounds;
- skip the declared response conversion;
- return the complete accepted bounded bytes and metadata.

It is not raw native response access.

Generate it only when the endpoint contains:

```rust
expose accepted_bytes;
```

At compile time, require the response mode to be accepted-byte capable through a sealed mode trait:

- buffered mode: use the bounded acquired bytes;
- discard mode: the explicit terminal may switch acquisition to bounded buffering, and this behavior must be documented/reflected;
- streaming mode: reject `expose accepted_bytes` with a declaration-local type/capability error.

No universal trait-bound method should appear on every request.

## 10.4 Streaming truth

For streaming output, document and preserve that receiving the stream value is not the same as fully consuming the remote body. Dropping the stream is local cancellation of further acquisition, not proof that the server canceled work.

No buffering terminal may silently turn an unbounded stream into memory retention.

---

# 11. HTTP methods and endpoint repeatability

## 11.1 Method surface

Support first-class keywords:

```text
GET POST PUT PATCH DELETE HEAD OPTIONS
```

Support custom methods through:

```rust
METHOD("PROPFIND") endpoint(...)
```

Validate the literal as an HTTP method token.

Remove the first-class `CONNECT` and `TRACE` keywords. Reject `CONNECT`, including `METHOD("CONNECT")`, because the current executor does not expose tunnel/upgrade semantics. Permit `METHOD("TRACE")` only as an explicit custom-looking declaration; apply the repeatability default below and document its unusual security semantics.

Do not ban GET or DELETE bodies merely by convention. The body specification remains authoritative. Do not claim HEAD body decoding is meaningful; document the recommended discard/header-derived response spec.

## 11.2 Repeatability clause

Add endpoint-only:

```rust
repeatability idempotent;
repeatability never;
```

Resolved method-derived default:

- `GET`, `HEAD`, `PUT`, `DELETE`, `OPTIONS`, and exact `TRACE`: `idempotent`;
- `POST`, `PATCH`, and unknown custom methods: `never`;
- `CONNECT`: unsupported.

An explicit clause may restrict or strengthen the default. It must appear in generated docs and descriptions.

`idempotent` means only that Concord is permitted to send another attempt after an eligible failure when all core gates and application policy allow it. It does not claim safety, cacheability, server correctness, concurrent deduplication, or automatic retry desirability.

`never` forbids policy-driven resend once an attempt may have reached the remote server. A purely local failure conclusively known to be pre-dispatch may be retried only if reconstruction is mechanically possible and policy permits.

Do not add `retry N` or a DSL retry classifier. Retry count, classifier, budget, backoff, and jitter remain application/runtime policy.

---

# 12. Retry, dispatch knowledge, and unknown outcome

## 12.1 Independent gates

A policy retry requires every relevant condition:

```text
configured retry policy permits
+ classifier recommends retry
+ failure stage is retry-eligible
+ endpoint repeatability permits a resend when dispatch may have occurred
+ body can be replayed when it was consumed/materialized
+ per-operation attempt maximum remains
+ shared retry budget admits
+ aggregate operation deadline remains
```

The retry classifier advises; it cannot bypass a core gate.

## 12.2 Failure stages

Represent enough execution state to distinguish at least:

```text
Preparation
BeforeDispatch
DuringDispatchOrUnknown
RejectedResponse
ResponseAcquisition
ResponseConversion
```

Expose a stable, caller-useful dispatch-knowledge classification rather than claiming to know remote commit state:

```rust
enum DispatchKnowledge {
    NotDispatched,
    PossiblyDispatched,
    ResponseReceived,
}
```

Naming may vary slightly, but the semantics may not. Do not call a transport timeout “remote failure.”

Rules:

- preparation/configuration/gate failures before native dispatch: `NotDispatched`;
- a transport failure after dispatch begins without a response: `PossiblyDispatched`;
- any received response head, including rejected status or later acquisition/conversion failure: `ResponseReceived`.

A response received still does not prove whether a side effect committed; it only narrows transport knowledge.

## 12.3 Retry stage policy

- Response conversion failures are not automatically retried. Resending cannot repair a deterministic decoder or accepted malformed representation.
- Response acquisition failures may be retried only for repeatable/replayable operations under classifier, budget, and deadline.
- Rejected transient statuses may be retried only under the same semantic gates.
- Nonrepeatable endpoints may retry only when `NotDispatched` is conclusive and any consumed body can be reconstructed.
- Every retry creates a new attempt identity, passes through rate admission again, and remains inside the same operation deadline.

Authentication recovery is a separate authority described below; do not count it as an application retry, but include it in total attempt/deadline/bound accounting.

---

# 13. Authentication and credential lifecycle

Authentication is already one of Concord’s strongest systems. Preserve its architecture while making syntax and ownership clearer.

## 13.1 Credential declarations

Use:

```rust
credentials {
    secret api_key;
    secret username;
    secret password;

    credential application_key = api_key(secret.api_key);
    credential static_bearer = bearer(secret.api_key);
    credential login = basic(secret.username, secret.password);
    credential acquired = endpoint authenticate;
    credential oauth = custom OAuthProvider;
}
```

A `secret` has no fake `: String` type. Its generated builder input is exactly the protected core/facade `SecretString` type.

Keep static API-key, bearer, Basic, endpoint-backed, and custom provider sources. Custom providers and secrets are required builder inputs. Do not implement optional secrets or optional custom-provider availability in the initial v2; absence must not become another hidden authentication branch.

Endpoint-backed source paths must resolve against the lexical client tree and be represented by stable endpoint IDs internally. Preserve cycle detection.

## 13.2 Authentication attachment

Use grouped syntax:

```rust
auth {
    bearer session recover on unauthorized;
    header "X-Api-Key" = application_key;
    query "token" = query_credential;
}

auth replace {
    authorization "Token" = oauth recover on unauthorized_or_forbidden;
}

auth off;
```

Rules:

- `auth { ... }` adds ordered local placements to inherited placements;
- `auth replace { ... }` starts from no inherited placements;
- `auth off;` clears authentication and is exclusive at the site;
- duplicate effective placement targets are errors;
- all Authorization forms remain mutually exclusive;
- generic private header placement cannot target `Authorization`, media, proxy, connection, cookie, or transport-owned fields;
- absence of `recover on` means no automatic recovery.

Supported recovery signals:

```text
recover on unauthorized
recover on unauthorized_or_forbidden
```

Remove `challenge never`; omission is the clear no-recovery state.

## 13.3 Runtime order and isolation

Preserve this security pipeline:

```text
public route/query/header shaping
→ request decision gate
→ body/prerequisite preparation as required
→ credential-context validation and material resolution
→ rate-limit admission
→ private credential placement
→ native dispatch
→ challenge attribution
→ at most one admitted recovery
→ response handling
```

The exact placement of body materialization may remain optimized by current architecture, but rate admission must not silently consume quota for a request that cannot be sent due to missing credential material. Private material may be resolved before admission but is placed only afterward.

Credentials and secrets must never enter:

- public expressions;
- logical/observable URLs;
- public headers;
- `CredentialFreeRequestPlan`;
- contract descriptions or generated Rustdoc;
- public hooks or completion events;
- rate keys;
- pagination cycle displays;
- public error/debug formatting.

Provider HTTP remains isolated, finite, deadline-bound, and without application client identity PEM. Endpoint-backed credential operations use the same managed executor. Credential slots remain versioned and single-flight, and stale acquisition/recovery cannot overwrite newer material.

Authentication recovery:

- occurs at most once for one request/page sub-operation; a pagination aggregate may contain one recovery on different pages, but every such resend remains bounded by page, attempt, rate, and aggregate-deadline limits;
- requires a declared recovery signal;
- requires mechanically replayable body state;
- stays inside the original operation deadline;
- reacquires rate admission before resending;
- does not become a general retry escape hatch;
- assumes the declared auth challenge means a resend is semantically acceptable. Document that services violating this assumption are incompatible with automatic recovery.

Client certificate identity remains `configure_http` transport configuration, not DSL authentication.

---

# 14. Timeouts, deadlines, and cancellation

## 14.1 Explicit root policy

Every generated client must declare either:

```rust
timeout 20s;
```

or:

```rust
timeout off;
```

Do not provide an invisible root operation-timeout default in the DSL. Descendants inherit by omission and may use `timeout D;` or `timeout off;`.

Durations remain positive integers with `ms`, `s`, `m`, or `h`, validated for overflow and bounded conversion.

## 14.2 Request controls

Every generated request retains:

```rust
.timeout(duration)
.disable_timeout()
.inherit_timeout()
```

`inherit_timeout()` removes only the request-level override and restores the endpoint’s resolved inherited selection.

## 14.3 One terminal, one aggregate deadline

A caller-visible terminal invocation allocates at most one absolute operation deadline when execution begins. That deadline covers:

- decision hooks/gates;
- body preparation and per-attempt materialization;
- credential acquisition and recovery;
- limiter waiting/admission;
- transport connection/send/receive;
- retry backoff;
- all retry and auth-recovery attempts;
- response acquisition and conversion;
- pagination controller work;
- every pagination page and page retry;
- aggregate pagination completion.

Most importantly, pagination must not create a fresh request deadline per page. Refactor the current pagination executor so the aggregate terminal creates the deadline once and passes the same absolute deadline into every page operation.

Check the deadline before and after trusted synchronous extension callbacks. A callback cannot be preempted by a check, so its public contract must require bounded/nonblocking work where appropriate.

## 14.4 Completion observation

The operation result is fixed before completion observation. A completion observer cannot transform a completed remote success into `Timeout`, `HookRejected`, or another caller result.

Separate decision authority from observation authority:

- a request/response gate may reject before the outcome is fixed and runs under the operation deadline;
- a completion observer receives a fixed bounded event, returns no decision, and cannot mutate the request or result.

Use a synchronous non-authoritative `CompletionObserver` contract for core v2. It must be documented as trusted and nonblocking. Applications needing asynchronous export should use an explicitly bounded adapter/sink (which may live in extras) with owned shutdown and drop/backpressure policy. Do not create an unbounded detached task in core.

Measure/check observer duration for diagnostics, but do not retroactively change the fixed outcome.

## 14.5 Cancellation truth

Dropping a request future stops further local polling. It does not prove that a dispatched remote operation was canceled or rolled back. Preserve this in Rustdoc and the error/dispatch-knowledge model.

Do not spawn hidden work that continues after request-future cancellation unless a named runtime service explicitly owns that lifecycle.

---

# 15. Pagination

## 15.1 Syntax

Use directional binding syntax:

```rust
paginate with OffsetPagination {
    offset -> endpoint.offset;
    page_size -> endpoint.count;
    tenant = client.tenant;
}
```

Meaning:

- `controller_field -> endpoint.value` grants the pagination adapter live authority to update exactly that endpoint-owned slot between pages;
- `controller_field = expression` initializes stable controller state and never grants mutation authority over the source;
- field order is source order and reflected;
- all controller fields are unique;
- all live destinations are unique.

Do not permit live bindings to client values, scope values, body input, secrets, credentials, or arbitrary request state. Endpoint slot mutation is private to the generated pagination adapter.

Stable initializers may use supported literals, const paths, and visible total direct owner references. Do not allow arbitrary expressions or indirect live bindings.

## 15.2 Caller controls and controller authority

A caller may configure a pagination-bound optional/defaulted endpoint value before calling `.paginate(...)`. Once the pagination terminal begins, the controller owns subsequent page-to-page values for the bound slots.

Do not remove ordinary request controls merely because a value is pagination-bound; document the handoff point.

Reject use of a pagination-mutated endpoint slot as a rate-key dimension in initial v2. Quota identity must not silently change with cursors/page numbers.

## 15.3 Page limits

Replace ambiguous `Take`/`HardCap` naming with:

```rust
PageLimit::Prefix(n)
PageLimit::CompleteWithin(nonzero_pages)
```

Semantics:

- `Prefix(0)` returns an empty aggregate immediately and performs no body preparation, provider acquisition, limiter admission, lifecycle hook, completion observer, or HTTP request;
- `Prefix(n)` returns the successfully collected prefix of at most `n` pages even if continuation remains;
- `CompleteWithin(n)` requires natural pagination completion within `n` pages; if continuation remains after page `n`, return a bounded pagination-limit error and no partial aggregate.

Pagination remains sequential, cycle-detecting, bounded, and all-or-error for failures. `Prefix` intentionally selects a successful partial domain result; it does not authorize returning partial data after an error.

## 15.4 Output and bounds

The declared endpoint output is one page’s output. Generated pagination requires that output to be consumable as an ordered item sequence and returns `Vec<Item>` (or the repository’s equivalent bounded aggregate type) after flattening accepted pages.

Preserve independent bounds for:

- page count;
- item count;
- cumulative accepted response bytes;
- cycle identity count and bytes;
- controller state and formatted values;
- retry/attempt count;
- aggregate deadline.

The response body must be buffered/page-capable, and any request body must explicitly satisfy page stability.

Cycle detection uses real credential-free request identity, including real values of redacted public fields internally. Observed diagnostics display only redacted forms. Credentials never participate.

Each actual network attempt and each recovery/retry reacquires rate admission under the same aggregate deadline.

---

# 16. Rate limiting

## 16.1 Responsibility split

Keep the core distinction:

```text
DSL: declares quota semantics and stable policy identity
runtime limiter: enforces admission/wait/rejection
```

The DSL does not implement token buckets, sleeping strategy, distributed coordination, or application retry policy.

Effective rate policies are additive across client, outer scopes, inner scopes, and endpoint. Descendants cannot disable inherited policies.

## 16.2 Profile syntax

Implement client-owned reusable profiles:

```rust
rate_limits {
    tenant_reads(tenant) {
        id "catalog.tenant.read";
        key [host, tenant];
        cost 1;
        allow 120 per 1m;
        allow 5_000 per 1h;
    }
}
```

Attach and bind every formal exactly once:

```rust
rate_limit tenant_reads(tenant = client.tenant);

rate_limit tenant_reads(tenant = client.tenant) {
    cost 2;
}
```

Profiles do not capture client/scope/endpoint values. Bare formal names in `key` refer only to declared profile formals.

Support anonymous attachment:

```rust
rate_limit {
    id "catalog.item.write";
    key [host, endpoint, client.tenant];
    cost 5;
    allow 30 per 1m;
}
```

`host` is the final logical host. `endpoint` is the stable endpoint ID. Key dimension order and names are semantic.

`cost` defaults to 1 if omitted, remains positive, and cannot exceed any declared window capacity. A policy has at least one positive window. Preserve current finite counts and duration/capacity bounds.

## 16.3 Canonical key conversion

Replace display-text rate keys with a dedicated public semantic trait such as:

```rust
pub trait RateLimitKeyValue {
    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateKeyError>;
}
```

The exact names may follow repository conventions, but the contract must provide:

- deterministic canonical bytes;
- unambiguous length/type framing;
- a finite writer bound;
- no dependence on `Debug` or human `Display` formatting;
- no access to credentials or native requests.

Provide coherent first-party implementations for commonly required primitives and structural standard types when they can be encoded without ambiguity. Generic `Option<T>` and `Vec<T>` implementations are acceptable at the trait layer because this is normal Rust trait semantics, not macro spelling recognition; encode variant/length boundaries explicitly.

A Concord-optional outer slot (`x?: T`) is illegal as a key dimension because policy presence would become data-dependent. A required payload whose type is `Option<T>` is allowed only if that payload implements `RateLimitKeyValue`; its `None` is an explicit domain value and must encode distinctly.

Profile formal binding and anonymous dimensions should accept direct total owner references, not `concat` or arbitrary formatting.

The final key must include dimension identity and length framing so examples such as `("ab", "c")` and `("a", "bc")` cannot collide.

## 16.4 Identity and enforcement

Preserve stable explicit policy IDs and deterministic derived IDs. Mark derived IDs as rename-sensitive in descriptions.

The same policy ID may be reused only when its ordered key-dimension schema and ordered windows are compatible. Reject duplicate effective use of the same ID on one endpoint plan.

A generated client whose contract has any effective rate policy must fail construction without a selected limiter. Do not silently install an allow-all limiter and do not add `without_rate_enforcement()` to production core. Tests may install an explicit test limiter from test support.

Limiter admission occurs once per actual network attempt, including retry and auth recovery, under the operation deadline. Resolved canonical key values do not appear in public plans, errors, hooks, completion, descriptions, or Rustdoc.

---

# 17. Planning, observation classes, and redaction

## 17.1 Rename the plan

Rename `SafeRequestPlan` to:

```rust
CredentialFreeRequestPlan
```

Remove the old public alias. The name makes one exact guarantee: credentials are structurally absent. It does not claim that all public application data is harmless.

## 17.2 Observation classes

Every declared ordinary value has one observation class:

```text
OrdinaryPublic
RedactedPublic
PrivateSecretOrCredential
```

`#[redact]` selects `RedactedPublic` for a public value. It does not change the real wire value.

Secrets and credentials are not redacted public values; they are structurally excluded from public representations.

## 17.3 Redaction propagation

For any observable host label, path segment, query value, or header value derived from a redacted value:

- use the real value for actual request shaping, internal pagination identity, and declared canonical rate keys;
- expose a fixed redacted variant/marker in plans, hooks, errors, debug output, and completion events;
- if `concat` depends on any redacted value, redact the entire resulting atom, including static prefixes/suffixes;
- never expose partial length, prefix, hash, or formatting failure text that leaks the value.

Prefer structured observed values, such as visible/redacted variants, over returning a raw string that callers may mistake for the exact wire value. `Debug` and `Display` of all observation types must be safe by construction.

## 17.4 Plan semantics

Provide:

```rust
request.plan() -> Result<CredentialFreeRequestPlan, PlanError>
client.plan(&unbound_endpoint) -> Result<CredentialFreeRequestPlan, PlanError>
```

`request.plan()` borrows the inert request; it does not consume it. Planning may perform bounded public formatting and route/header/query validation. It must not:

- perform network I/O;
- call credential providers;
- place authentication;
- acquire a limiter;
- invoke execution hooks;
- materialize body bytes or inspect body values;
- allocate an operation/attempt execution lifecycle;
- mutate credential or rate state.

A plan may include body kind, declared media type, and statically/purely known length metadata only through a body-spec metadata contract that does not inspect or expose the body value.

The plan should expose structured endpoint identity, method, observed/redacted logical URL and public headers, timeout selection, repeatability, body/response kinds, declared rate-policy identities, and available terminal capabilities. It must explicitly indicate that redaction may make observed URL/header representations differ from actual public wire values.

Planning and execution must use the same resolved pure shaping representation. Add equivalence tests rather than duplicating URL/query/header construction.

---

# 18. Canonical contract description and semantic diff

## 18.1 One resolved semantic IR

Refactor the macro into clear stages:

```text
raw AST
→ name/owner resolution
→ canonical semantic IR
→ validation
→ all projections
```

The canonical IR should contain stable internal identities and facts similar to:

```text
ApiIr
ClientIr
ScopeIr
EndpointIr
OwnerId
ValueId
ValueIr {
    payload_type,
    presence,
    initializer,
    observation,
    owner,
    docs,
    source_span
}
ExpressionIr
EmissionIr
AttachmentIr
AttachmentOrigin
QueryPatchIr
HeaderPatchIr
AuthIr
PaginationIr
RatePolicyIr
ResponseCapabilityIr
GeneratedSurfaceIr
```

This list is conceptual, not a required exact Rust struct layout. The required property is that emitters consume resolved identities and semantic operations rather than re-parsing tokens or reconstructing meaning independently.

Remove or merge duplicate semantic models such as a separate documentation model that can drift from code emission.

## 18.2 Public descriptions

Generate:

```rust
CatalogApi::contract() -> &'static ContractDescription
request.description() -> &'static EndpointDescription // or a stable endpoint view
unbound_endpoint.description() -> &'static EndpointDescription
```

Use private fields, typed accessors, and `#[non_exhaustive]` public enums/structures where downstream matching should remain evolvable. Do not immediately promise a stable JSON wire format for the descriptor.

The description must expose, without runtime values or secrets:

- client/scope/endpoint tree and stable IDs;
- author docs and deprecation;
- value owners, names, payload type spelling, presence, default kind, and observation class;
- local and effective route/query/header operations with source origin;
- body input type and body-spec type;
- response output type, response-spec type, acquisition category/capabilities when trait-declared;
- auth placement categories and recovery signals, never credential material;
- timeout selection;
- resolved repeatability and whether it is explicit or method-derived;
- rate profile/policy identities, formal bindings, dimension names, cost, and windows, never values;
- pagination controller type, live bindings, stable initializers, and bounds category;
- generated terminal capabilities;
- whether endpoint/policy IDs are explicit or derived/rename-sensitive.

Distinguish:

1. statically declared facts;
2. trait-declared capabilities;
3. opaque trusted extension behavior;
4. runtime-bound facts.

Do not invent static precision for arbitrary extension code. Represent it as opaque where necessary.

## 18.3 Local/effective views and origins

For inherited families, make both local declarations and effective resolved behavior inspectable. Every effective entry should retain an origin identifying the client, scope, or endpoint that introduced it.

This is required for reviewer locality and useful diagnostics.

## 18.4 Semantic diff

Add an internal/`xtask` semantic contract snapshot and diff facility based on the canonical IR or descriptor. It must classify at least:

```text
caller API change
value/domain meaning change
wire route/query/header change
security/auth change
runtime-policy/repeatability/deadline change
capability/terminal change
identity/rate-schema change
documentation-only change
```

Use a deterministic tool-private serialization if needed. Do not expose that serialization as a promised public format yet.

Add fixture-based tests proving deterministic output and representative classifications.

---

# 19. Generated Rust API and caller experience

## 19.1 Client builder

Keep high-frequency domain inputs and credentials directly visible on the generated builder. Keep transport and operational configuration grouped:

```rust
CatalogApi::builder()
    .set_tenant(tenant)
    .bootstrap_key(secret)
    .retry(policy)
    .configure_http(|http| ...)
    .configure_runtime(|runtime| ...)
    .build()
```

Do not flatten every HTTP/runtime option into the generated builder.

Use one canonical retry spelling: generated builder `.retry(policy)` and runtime builder `.retry(policy)` if the runtime API exposes the same operation.

Generate `Client::new()` only when no required client values, secrets, custom providers, limiter selection, or other required runtime choices are missing. The docs should teach `builder()` as the canonical path.

Build errors must aggregate or deterministically report missing inputs using stable names and categories. No panic for ordinary missing configuration.

## 19.2 HTTP versus runtime configuration

Preserve two cohesive configuration surfaces:

```text
HttpConfigBuilder
  TLS, proxy, protocol, connection, transport timeouts

RuntimeConfigBuilder
  finite body/response/stream/record/pagination/provider limits,
  retry, limiter, decision gates, completion observer, debug/diagnostics
```

Use intention-revealing security methods such as `require_https()` / `allow_http()` instead of ambiguous booleans for trust decisions. Ordinary low-risk socket booleans may remain when established and clear.

Proxy configuration must remain safe and unable to inspect endpoint credentials or mutate native requests. Provider HTTP policy remains isolated from application HTTP policy.

Document clone semantics: client clones share transport, immutable runtime selection, versioned credential slots, limiter, retry/budget handles, and hooks; derived scope/endpoint values are independent immutable snapshots. Independent builds have separate security/runtime identities unless the application deliberately supplies shared service handles.

## 19.3 Scope and request usage

Generated direct usage must be domain-shaped and capability-shaped:

```rust
let regional = client.regional(region).with_language(language);
let item = regional.item(id).await?;
```

Do not copy ancestor controls onto endpoint requests. Configure an optional/defaulted scope value on the scope before selecting a child endpoint. This keeps ownership visible.

A request is `#[must_use]`, body-free in `Debug`, and inert until a terminal is polled.

Do not expose generic public mutation such as arbitrary path/query/header setters. Runtime-defined endpoints are the explicit dynamic-contract surface.

## 19.4 Unbound endpoint tree

Generate an isomorphic unbound tree rooted at:

```rust
CatalogApi::unbound()
```

The unbound tree captures scope and endpoint values/body input but no client runtime, client values, credentials, transport, limiter, or hooks.

Provide:

```rust
client.request(endpoint_intent) -> BoundRequest
client.plan(&endpoint_intent) -> Result<CredentialFreeRequestPlan, PlanError>
```

The endpoint intent is storable, passable, describable, and inert. Do not generate flat leaf constructors that repeat all ancestor parameters.

Property law:

```text
direct construction with values V
== unbound tree construction with values V, then client binding
```

for endpoint identity, effective values, route, public head, auth declarations, timeout, rate policies, pagination bindings, terminals, and executor path.

## 19.5 Public type hygiene

- Keep generated fields private.
- Use narrow re-exports from the facade.
- Do not require a broad prelude for ordinary generated use.
- Keep internal support/bridge symbols hidden.
- Avoid generic parameters propagating through caller-visible types unless they encode a real capability.
- Implement `Debug`, `Clone`, and other common traits only when their semantics are true; never leak body values or secrets.

---

# 20. Errors, gates, hooks, and diagnostics

## 20.1 Error surface

Organize public errors around caller decisions, not the implementation module tree. Preserve source chains in a trusted/private diagnostic channel while exposing stable categories such as:

- invalid configuration/request construction;
- credential unavailable/authentication failure;
- rate/admission rejection;
- application policy/gate rejection;
- operation timeout;
- transport failure;
- rejected HTTP response;
- response acquisition failure;
- response conversion failure;
- pagination limit/cycle/controller failure;
- internal invariant defect.

Do not expose raw provider, native transport, body, credential, or redacted public data in `Display`/`Debug`.

Every execution error should expose safe structured context where applicable:

- endpoint ID and method;
- operation ID;
- attempt number/ID;
- page number/aggregate ID;
- stable stage/category;
- `DispatchKnowledge`;
- source error through a trusted accessor where safe.

Do not produce an enormous exhaustive enum that embeds every dependency error type. Use stable categories plus source/context.

## 20.2 Decision gates versus observers

Replace one ambiguous “hooks can observe and reject” model with explicit authority:

```text
RequestGate / ResponseGate
  may reject at a named pre-outcome phase

CompletionObserver
  receives a fixed bounded outcome and cannot reject or mutate it
```

Suitable gate names include:

```rust
before_send(...)
before_response_handling(...)
```

A gate receives only a credential-free/redacted lifecycle view, not native request/response, body values/bytes, credentials, limiter state, or arbitrary endpoint mutation.

A completion observer receives bounded IDs, timing, attempts/pages, safe outcome class, and redacted/credential-free context. It receives no decoded output or response body unless a future dedicated observer explicitly earns that authority.

Do not pass a universal application context. Implementations may close over their own application services.

## 20.3 Trusted diagnostics

Where operators need the raw native source chain, provide an explicitly trusted diagnostic sink/configuration that is separate from ordinary plan/hook/error formatting. It must still exclude credentials and body content by construction where possible and clearly document application responsibility for any public application values.

All diagnostic strings, labels, histories, and retained events remain bounded.

## 20.4 Macro diagnostics

UI diagnostics are a product surface. Add targeted errors for:

- old v1 spellings and migration replacements;
- duplicate/generated-name collisions;
- invalid owner references and ambiguous scope identifiers;
- optional values in total contexts;
- `each` on an invalid expression;
- body/result associated-type mismatch assertion sites;
- reserved headers and media ownership;
- invalid query/header/auth merge combinations;
- pagination destination authority;
- pagination-controlled rate dimensions;
- missing/duplicate rate formal bindings;
- unsupported accepted-bytes response mode;
- CONNECT/tunnel semantics;
- unknown attributes and clauses.

Diagnostics must name the declaration and violated semantic rule and must never include runtime values.

---

# 21. Runtime-defined endpoints

Keep runtime-defined endpoints as the dynamic-contract escape route, not a native HTTP escape route.

They must:

- execute through the same core executor;
- use the same value/route/query/header/auth/rate/repeatability/deadline/redaction semantic structures where applicable;
- expose the same credential-free plan model;
- produce a contract/endpoint description owned by the runtime endpoint value;
- use `subdomain`, path patching, query ordered-multimap patching, header patching, response specs, explicit repeatability, and explicit accepted-byte capability vocabulary aligned with v2;
- retain first-error/error-latch behavior during builder configuration;
- remain bound to one client security/runtime context when using generated credentials;
- never expose the native `reqwest::Request` or `Response` for mutation;
- never mutate generated client/scope values through generic APIs.

Runtime endpoint pagination, if retained, must use the same aggregate deadline, bounds, controller authority, and all-or-error semantics.

Do not duplicate a second implementation of public shaping; reuse canonical core structures generated endpoints also use.

---

# 22. Extension boundaries

Keep these public semantic extension roles, with precise authority documentation:

| Extension | May see | May decide | Must not decide |
|---|---|---|---|
| `EncodeBody` | typed input | bounded complete encoding | retry, auth, native request |
| `DecodeBody` | bounded accepted bytes/head subset | typed conversion | acquisition bounds, native response |
| `RequestBodySpec` | body input and narrow body context | media metadata, preparation, replay/page-stability contract | route, auth, rate, retry policy |
| `ResponseSpec` | accepted semantic head and sealed mode body | accepted statuses, media, conversion | native acquisition mechanics/deadline |
| `RecordDecoder` | bounded chunks and its state | framing/record decode | network polling/global buffer |
| `CredentialProvider` | provider context and own state | acquire/recover material | placement target, endpoint mutation |
| `PaginationController` | accepted page result/metadata and own state | next bound values/completion | arbitrary request mutation/execution order |
| `RateLimiter` | resolved policy and opaque canonical key | admit/wait/reject | auth, endpoint semantics |
| `RetryClassifier` | safe failure/attempt context | recommendation | bypass repeatability/replay/budget/deadline |
| gate | safe lifecycle view | reject at one named phase | native/private mutation |
| completion observer | fixed bounded event | record evidence | alter outcome |

Keep response acquisition modes sealed. Keep codecs, providers, pagination controllers, limiters, classifiers, gates, and observers public only where their obligations are stable and testable.

Do not add a trait for each DSL token. Static route/value/query operations belong in generated concrete data. Traits exist only at real runtime variation boundaries.

---

# 23. Macro, bridge, core, extras, and facade responsibilities

## 23.1 `concord-macros`

Own:

- v2 parsing and old-syntax migration diagnostics;
- source spans and docs;
- owner/name resolution;
- canonical semantic IR;
- inheritance/patch resolution;
- structural validation and generated-name collision analysis;
- capability surface computation;
- generated Rust code;
- compile-time associated-type assertions;
- static contract descriptions;
- deterministic semantic snapshots/diff input.

The macro must not recognize concrete extras/body/response/provider type names.

## 23.2 Hidden generated bridge

Own the smallest stable handoff from generated code to core:

- typed endpoint/request construction;
- resolved public shaping operations;
- value accessors/lenses required for pagination;
- static IDs and descriptions;
- terminal capability markers;
- body/response spec types;
- resolved auth/rate/deadline/repeatability metadata.

Keep it hidden and version-checked. Bump the shared contract version to 32.

Do not expose parser concepts through the bridge.

## 23.3 `concord-core`

Own:

- client runtime snapshot and security context;
- bounded public formatting and request-head construction;
- credential-free planning and redaction types;
- body preparation/materialization;
- one managed executor and execution ordering;
- operation/attempt/page IDs and aggregate deadlines;
- credential slots, providers, private placement, and recovery;
- rate policy resolution handoff and limiter invocation;
- retry gates, budgets, and dispatch knowledge;
- response acquisition, status handling, and conversion orchestration;
- pagination loop, cycle detection, and aggregate bounds;
- stable public error categories;
- gates, completion observation, and trusted diagnostics;
- runtime-defined endpoint semantic builders.

Core must not parse DSL syntax or infer behavior from Rust type names.

## 23.4 `concord-extras`

Own optional first-party implementations:

- JSON and other codecs/specs;
- OAuth/provider implementations;
- CSV/NDJSON/record helpers;
- multipart and stream helpers;
- rate limiter implementations;
- bounded completion-export adapters if provided.

Extras must satisfy core contracts rather than receive special recognition from the macro.

## 23.5 `concord` facade

Own:

- curated public re-exports;
- feature composition;
- macro entry point;
- generated bridge path;
- user-facing module organization.

Keep ordinary usage discoverable without exposing core internals or requiring users to depend directly on macro/core crates.

---

# 24. Implementation sequence

Implement in semantic slices. Keep the workspace buildable at meaningful checkpoints and update tests with each slice. Do not postpone all documentation/tests until the end.

## Phase 0 — Characterize and establish v2 constitution

1. Record current relevant behavior with focused tests before changing mechanisms, especially executor ordering, credential isolation, bounds, pagination, retry, and response acquisition.
2. Rewrite `docs/design_invariants.md` for v2, preserving the strong invariants and removing the freeze rule.
3. Add a short v2 architecture decision document containing the product identity, non-goals, projection laws, and chosen migration posture.
4. Set an internal implementation branch/version marker if useful, but final workspace version is `2.0.0`.

## Phase 1 — Canonical semantic IR

1. Separate raw syntax AST from resolved semantic nodes.
2. Introduce stable owner/value/endpoint/attachment identities.
3. Move inheritance, origin tracking, generated-surface computation, and collision analysis into the resolved stage.
4. Make current emitters consume the IR before changing every syntax rule, where feasible.
5. Add deterministic IR snapshot tests.
6. Remove duplicated documentation-specific semantic reconstruction.

Exit criterion: one resolved endpoint node can feed code emission, docs, description, and diagnostics without new token lookup.

## Phase 2 — Explicit value algebra

1. Add `?` grammar.
2. Add `#[default]` and `#[redact]`.
3. Remove exact `Option`/`Vec` recognition and `ValueShape` token semantics.
4. Implement exact stored types and control-method matrix.
5. Implement retained default timing and reset laws.
6. Add `each` as placement cardinality.
7. Add all alias/qualification/nested-option compile tests.

Exit criterion: no macro code asks whether a Rust type is `Option` or `Vec` to determine language semantics.

## Phase 3 — Source topology and ownership

1. Move children into the client grammar.
2. Rename `vars` to `values`.
3. Implement `client`, named-scope, and `endpoint` namespaces.
4. Add scope-identifier visibility/uniqueness validation.
5. Generate the isomorphic unbound tree.
6. Add direct/unbound construction equivalence tests.

Exit criterion: source, direct generated use, unbound use, modules, and descriptions follow the same tree.

## Phase 4 — Signature truth

1. Implement `body Input using Spec`.
2. Implement `-> Output using Spec`.
3. Rename `ResponseBodySpec` to `ResponseSpec` across crates.
4. Generate associated-type equality assertions.
5. Update endpoint required-argument generation.
6. Add endpoint-specific diagnostics and docs.

Exit criterion: every visible parameter and arrow type is exactly the caller’s input/output type.

## Phase 5 — Attachment-language cleanup

1. Rename `fmt` to `concat`.
2. Rename/redefine `host` as composable `subdomain`.
3. Implement path algebra exactly.
4. Implement query `=`, `+=`, `remove`, `replace`, and `off` as an ordered-multimap patch.
5. Implement header assignment/removal with case-insensitive identity.
6. Reserve `Accept`/`Content-Type` and add response-owned `accept`/`accept off`.
7. Group auth syntax and change recovery wording.
8. Replace rate syntax and canonical key conversion.
9. Preserve origin metadata through all resolved operations.

Exit criterion: every inheritance family has one explicit algebra and tests.

## Phase 6 — Runtime truth fixes

1. Add resolved endpoint repeatability and failure-stage/dispatch knowledge.
2. Gate retries on semantic repeatability plus body replayability, policy, budget, stage, and deadline.
3. Allocate one aggregate pagination deadline and thread it through every page/attempt/callback.
4. Replace `response_bytes` with endpoint-declared `accepted_bytes` capability.
5. Add redacted public-value propagation and rename the plan.
6. Separate decision gates and completion observation authority.
7. Ensure operation/page/attempt IDs and error context remain truthful.
8. Align runtime-defined endpoints.

Exit criterion: fluent usage never promises more than execution provides.

## Phase 7 — Contract reflection and semantic diff

1. Generate `ContractDescription` and endpoint views from the IR.
2. Include local/effective origins and capability classifications.
3. Generate Rustdoc from the same facts.
4. Add deterministic semantic snapshots and `xtask` diff classification.
5. Test code/docs/description consistency.

## Phase 8 — Migration, examples, and public documentation

1. Update README, quick start, overview, mental model, full spec, generated-client guide, values/requests, auth, responses, pagination, rate limits, planning, runtime, extensions, and all examples.
2. Add `docs/migration_v1_to_v2.md` with a mechanical mapping and semantic warnings.
3. Replace all fixtures and compile tests with canonical v2 syntax.
4. Add old-syntax UI tests for targeted errors.
5. Remove stale v1 names outside migration docs and diagnostic tests.

## Phase 9 — Hardening and deletion

1. Delete obsolete AST variants, emit helpers, compatibility aliases, duplicate code paths, and stale docs.
2. Audit public exports and generated names.
3. Run full feature/MSRV/docs/security/performance verification.
4. Fix warnings; do not suppress them broadly.
5. Confirm every new mechanism has tests for its negative space.

---

# 25. Expected file-level work

Use the actual repository structure, but expect changes in at least these areas.

## `crates/concord-macros`

Syntax:

- `src/syntax/ast.rs`
- `src/syntax/keyword.rs`
- `src/syntax/values.rs`
- `src/syntax/declaration.rs`
- `src/syntax/request.rs`
- `src/syntax/credentials.rs`
- `src/syntax/pagination.rs`
- `src/syntax/rate_limits.rs`
- attribute/doc parsing helpers

Semantic resolution:

- `src/semantic/model.rs`
- `src/semantic/mod.rs`
- `src/semantic/inheritance.rs`
- `src/semantic/symbols.rs`
- `src/semantic/surface.rs`
- `src/semantic/auth.rs`
- `src/semantic/pagination.rs`
- `src/semantic/rate_limits.rs`
- `src/semantic/head_limits.rs`
- new canonical IR/origin/diff modules as appropriate

Emission:

- `src/emit/client.rs`
- `src/emit/scopes.rs`
- `src/emit/endpoints.rs`
- `src/emit/requests.rs`
- `src/emit/shaping.rs`
- `src/emit/docs.rs`
- `src/emit/descriptions.rs`
- `src/emit/assertions.rs`
- `src/emit/bridge.rs`
- `src/emit/mod.rs`

Do not preserve semantic lookup in multiple emitters.

## `crates/concord-core`

Likely areas:

- `src/endpoint.rs` and/or a new `contract/` module;
- request head/options/endpoint/preparation modules;
- execution orchestration and tests;
- pagination execution/controller/limits/errors;
- operation deadline and identity;
- body spec/prepared-body traits;
- response spec/modes/handlers;
- auth slots/providers/placement/recovery;
- rate-limit types/key writer/limiter;
- redaction and observed-value types;
- public errors and dispatch knowledge;
- gates/hooks/completion/diagnostics;
- runtime endpoint builders.

## Facade, extras, examples, tools, docs, and tests

Update:

- `crates/concord` exports and UI harness;
- all `concord-extras` implementations of renamed/changed traits;
- `concord-examples` declarations and usages;
- `concord-test-support` safe fixtures and limiters;
- `tools/xtask` fixtures, policy checks, MSRV UI, and semantic diff;
- performance fixtures;
- every guide and source-linked documentation snippet;
- workspace package metadata/version and generated contract version.

This map is guidance, not permission to make unrelated broad changes.

---

# 26. Migration requirements

Create `docs/migration_v1_to_v2.md` containing at least this mapping:

| v1 | v2 |
|---|---|
| children after `client` | children nested inside `client { ... }` |
| `vars { ... }` | `values { ... }` |
| `vars.x` | `client.x` |
| `scope.x` | `actual_scope_name.x` |
| `ep.x` | `endpoint.x` |
| optionality inferred from `Option<T>` | explicit `name?: T`; written type remains opaque |
| `= default` | `#[default]` |
| vector repetition inferred from `Vec<T>` | explicit `each endpoint.values` at placement |
| `fmt[...]` | `concat [...]` |
| replacing `host` labels | composable `subdomain`, `subdomain replace`, or `subdomain off` |
| query append-only `=` | replacement `=`, append `+=`, `remove`, `replace`, `off` |
| public `Accept` | endpoint `accept` / `accept off` |
| `#[body] payload: BodySpec` | `body payload: Input using BodySpec` |
| `-> ResponseSpec` | `-> Output using ResponseSpec` |
| `ResponseBodySpec` | `ResponseSpec` |
| repeated `auth ...;` clauses | grouped `auth { ... }` |
| `challenge ...` | `recover on ...`; omission means no recovery |
| old pagination `bind` syntax | directional `controller -> endpoint.slot` |
| old rate `rule/by/use` syntax | profile formals, `key`, `allow ... per ...`, direct attachment |
| `response_bytes()` | endpoint-declared `accepted_bytes()` |
| `SafeRequestPlan` | `CredentialFreeRequestPlan` |
| `PageLimit::Take/HardCap` | `Prefix/CompleteWithin` |
| implicit retry semantic permission | resolved `repeatability` |

Explain the semantic traps, especially:

- `x: Option<T>` is required in v2 unless `x?` or a default is declared;
- `Vec<T>` no longer repeats query pairs without `each`;
- optional query assignment now removes inherited values when absent;
- scope owner references become explicit;
- timeout on pagination now covers the whole terminal;
- policy retry is stricter because repeatability is explicit;
- plans may redact declared public data and are credential-free, not universally safe.

Old syntax diagnostics should be targeted where unambiguous:

- `vars` → `values`;
- `fmt` → `concat`;
- `host` → `subdomain`;
- `= default` → `#[default]`;
- `#[body]` → `body ... using ...`;
- missing response `using`;
- old auth/pagination/rate syntax;
- children outside client;
- old plan/terminal/type names where compile diagnostics can help.

Do not attempt to infer whether every syntactic `Option<T>` or `Vec<T>` intended v1 optional/repeated semantics. Document and optionally add an `xtask` migration audit that reports occurrences requiring human review without changing v2 semantic resolution.

---

# 27. Required tests and evidence

Testing must prove semantic claims, not only parser branches.

## 27.1 Parser and compile diagnostics

Add pass/fail UI tests for:

- `?` on client/scope/endpoint ordinary values;
- arbitrary opaque Rust type forms, aliases, leading `::`, qualified paths, references, generics, and function types where otherwise legal;
- `#[default]`, `#[redact]`, attribute combinations, and unsupported attributes;
- mutually exclusive default forms;
- children inside client and old topology error;
- owner-qualified lookup, unavailable owner, duplicate scope identifier in an ancestor chain;
- `concat` grammar and rejection of arbitrary expressions;
- optional values rejected in total contexts;
- `each` direct-reference requirement;
- query operators and invalid combinations;
- header reserved names and case-insensitive collisions;
- body syntax restrictions;
- output/result syntax and associated-type mismatches;
- repeatability syntax/method defaults/CONNECT rejection;
- grouped auth and recovery syntax;
- pagination live destination restrictions;
- rate formals/key/cost/window/schema restrictions;
- accepted-byte exposure on valid/invalid modes;
- generated-name collisions;
- old v1 migration diagnostics.

## 27.2 Value/type matrix

Compile and runtime tests must prove:

```text
x: T
x?: T
x: Option<T>
x?: Option<T>
x: Vec<T>
x?: Vec<T>
aliases and fully qualified equivalents
```

Verify exact generated constructor arguments and setter signatures, including nested `Option<Option<T>>`.

Prove that changing only a type alias does not change Concord semantics.

## 27.3 Default lifecycle

Use observable `Default` implementations/counters to prove:

- client default once per build;
- scope default once per scope construction;
- endpoint default once per endpoint construction;
- retries/recovery/pages do not reevaluate;
- snapshot/request reset does not reevaluate;
- builder reset causes the next build to evaluate once;
- optional default `Some(default)`, clear `None`, and reset differ correctly.

## 27.4 Cardinality and query algebra

Prove:

- `Vec<T>` is not special without `each`;
- aliases and custom iterable types work with `each` through traits;
- optional `each` handles None/empty/nonempty correctly;
- deterministic iteration/query order;
- assignment removes inherited duplicates then appends at statement position;
- append retains inherited values;
- absent optional assignment versus append differ;
- repeated local operations for one key follow source order;
- `replace`/`off` semantics;
- query key case sensitivity;
- pure model/property tests compare expected ordered pairs to generated/core shaping.

## 27.5 Route and headers

Test:

- subdomain prepend/replace/off across nested scopes;
- IP base rejection when subdomain is used;
- DNS and total-host first-excess bounds;
- base-path preservation;
- path append/replace/off and invalid segments;
- one encoding pass;
- header case-insensitive assignment/removal;
- absent optional header assignment removes inherited field;
- public/private/media collision rejection;
- response-owned Accept override and suppression.

## 27.6 Body and response truth

Test:

- direct argument type is declared body input;
- await output is declared arrow type;
- body/response associated-type mismatch diagnostics are endpoint-local;
- body cannot be optional/defaulted/referenced publicly;
- body preparation/replay/page stability retains current ordering and bounds;
- response modes remain sealed;
- `response()` universally exists;
- `accepted_bytes()` exists only with explicit exposure;
- buffered/discard behavior and bounded acquisition;
- streaming exposure rejection;
- accepted bytes preserves status/auth/Accept while bypassing conversion.

## 27.7 Direct/unbound/description consistency

For representative nested APIs, prove:

- direct and unbound-bound requests have identical endpoint ID, values, public head, auth metadata, timeout, repeatability, rate policy, pagination bindings, capabilities, and executor events;
- unbound construction performs no runtime effects;
- client planning an unbound intent performs no credentials/limiter/body/hook effects;
- generated docs and descriptions report the same source facts and owners;
- deterministic contract snapshots.

## 27.8 Retry matrix

Build a deterministic matrix across:

```text
failure stage
× dispatch knowledge
× endpoint repeatability
× body replayability
× classifier decision
× per-operation max
× shared budget
× remaining deadline
```

At minimum prove:

- nonrepeatable + possibly dispatched never policy-retries;
- idempotent + one-shot body cannot resend;
- idempotent + replayable + admitted transient failure may retry;
- pre-dispatch local failure does not require semantic resend permission but still requires reconstructability;
- response conversion never policy-retries;
- every retry reacquires limiter and gets a new attempt ID;
- budget and deadline cap the sequence;
- dispatch knowledge in final error is truthful.

## 27.9 Authentication/security noninterference

Use sentinel secrets, credential material, redacted values, and public values. Assert across plans, descriptions, errors, `Debug`, hooks/gates, completion, limiter observation, pagination diagnostics, and semantic snapshots that:

- secrets/credentials never appear;
- redacted public values never appear in ordinary observation;
- concat propagates redaction to the whole atom;
- actual server/native request receives the real public/redacted and private values in the correct places;
- private placement occurs after limiter admission;
- cross-client credential context is rejected before private/native work;
- one recovery maximum and versioned-slot stale-write protection remain;
- provider HTTP remains isolated and bounded.

Do not rely only on “does not contain token” string tests; also test structured accessors do not expose it.

## 27.10 Deadline and pagination

Use deterministic time to prove:

- one request terminal uses one deadline;
- pagination uses one aggregate deadline across pages, retries, recovery, waits, and backoff;
- no per-page deadline reset;
- `Prefix(0)` performs no runtime side effects;
- `Prefix(n)` returns intentional prefix only on successful pages;
- `CompleteWithin` errors if continuation remains;
- page errors never leak partial aggregate;
- cycle detection uses actual internal values but reports redacted observation;
- page/item/cumulative-byte first-excess behavior;
- controller cannot mutate unauthorized state;
- page-stable body requirement;
- rate admission happens per actual attempt.

## 27.11 Rate limiting

Test:

- profile formals all bound exactly once;
- no captured values;
- anonymous/named syntax;
- additive inheritance and no disable;
- missing limiter fails client build;
- key canonical framing prevents concatenation collisions;
- structural Option/Vec canonical encoding is distinct and bounded where implemented;
- outer optional values rejected;
- pagination-mutated values rejected as dimensions;
- same ID/schema compatibility checks;
- key values absent from ordinary observation;
- limiter wait obeys aggregate deadline.

## 27.12 Hooks, completion, and errors

Test:

- gates may reject only at their named phase;
- gate errors classify as policy rejection and are NotDispatched when appropriate;
- completion observer receives exactly one fixed final event per terminal according to documented rules;
- observer cannot alter returned success/failure;
- observer output/data bounded and redacted;
- no universal/native/private context access;
- operation/attempt/page IDs correlate correctly;
- public errors remain actionable and sanitized.

## 27.13 Runtime endpoint equivalence

Test that a runtime-defined endpoint with equivalent semantics:

- uses the same executor event order;
- uses the same planner/redaction/head structures;
- obeys the same retry/deadline/auth/rate/response bounds;
- cannot access native request mutation;
- retains error latch behavior.

## 27.14 Bounds and negative space

For every new list, string, atom, key, description collection, and event, test exact limit and first excess.

Compile-fail or API-shape tests must prove absence of:

- native request/response mutation;
- generic endpoint path/query/header setters;
- universal accepted-byte terminal;
- custom response modes;
- retry-classifier bypass;
- pagination mutation of client/scope/body;
- secret use in public shaping/rate keys;
- old public aliases;
- silent rate enforcement bypass.

## 27.15 User-facing evidence

Add small, medium, and nested examples showing:

- create a client;
- configure optional/defaulted values;
- add/call a simple endpoint;
- body/result specs;
- optional and repeated query fields;
- auth acquisition/use/recovery;
- planning/redaction;
- pagination;
- rate limiting;
- unbound endpoint transport;
- one custom body/response/provider/controller/limiter extension.

Generated Rustdoc must make each persona’s first task possible without reading macro internals.

---

# 28. Verification gates

Run and pass the repository’s complete gate:

```text
just fmt
just check
just lint
just test
just ui
just msrv
just fixtures
just docs
just deny
just workflow
just analyzer
just bench
just verify
```

`just verify` is the final required gate. Do not report completion with known failing subcommands.

Also run a stale-language audit. Outside migration documentation and explicit migration-diagnostic tests, there should be no production/documentation use of:

```text
vars
vars.
scope.
ep.
fmt[
host [
#[body]
= default
ResponseBodySpec
response_bytes
SafeRequestPlan
PageLimit::Take
PageLimit::HardCap
challenge unauthorized
rate_limit use
rule ... by
```

Use context-aware searches so unrelated English words do not create false failures.

Preserve the feature matrix and MSRV. Do not add a dependency when the standard library/current workspace can provide the needed mechanism. Run the existing benchmark compile gate and compare representative overhead before/after; investigate material regressions rather than hiding them.

---

# 29. Forbidden shortcuts

Do not solve this task by any of the following:

- retaining syntactic `Option`/`Vec` detection behind a helper with a new name;
- implementing `?` while also unwrapping written `Option<T>`;
- making setters accept broad `Into` conversions;
- parsing v2 but emitting v1-generated signatures;
- adding a second documentation/reflection model separate from code generation;
- placing arbitrary Rust expressions in route/query/header/rate declarations;
- exposing native `reqwest` request/response callbacks;
- adding a universal context to extensions;
- allowing retry classifiers to bypass core safety gates;
- resetting pagination deadlines per page;
- exposing accepted bytes through an accidental trait bound on every request;
- calling a credential-free representation “safe” while it may contain public sensitive data;
- redacting only formatted strings while structured accessors still expose values;
- silently accepting rate-limited clients without a limiter;
- allowing descendants to disable inherited rate policy;
- using `Display` as a durable rate-key encoding;
- hiding body/result associated types behind spec-looking signature positions;
- flattening explicit endpoint constructors;
- preserving old public names as permanent aliases;
- suppressing warnings or deleting negative tests to obtain a green build;
- broad rewrites unrelated to the semantic migration.

---

# 30. Definition of done

Concord v2 is complete only when all of these statements are true:

1. A Rust type’s spelling never changes Concord optionality or cardinality.
2. `?`, defaults, and placement multiplicity are independent and exhaustively tested.
3. `set_*` replaces the exact effective type; `with_*`, `clear_*`, and `reset_*` have precise laws.
4. Source containment, direct usage, unbound usage, generated modules, and descriptions are the same domain tree.
5. Every public reference names its actual value owner.
6. Body input and result output types are truthful in the declaration.
7. `concat`, subdomain, path, query, header, auth, timeout, rate, and pagination each have one documented inheritance/patch algebra.
8. Media headers have one owner.
9. Retry requires both semantic repeatability and mechanical replayability where a resend is possible.
10. A pagination terminal has one aggregate deadline.
11. Authentication remains private, ordered, versioned, bounded, and limited to one recovery.
12. Rate keys are canonical and rate declarations cannot silently run unenforced.
13. Plans are credential-free, redaction-aware, inert, and share shaping logic with execution.
14. Byte access is an explicit endpoint capability with bounded truthful behavior.
15. Errors expose stage, IDs, and dispatch knowledge without secrets or redacted values.
16. Decision gates and completion observers have different authority.
17. Runtime-defined endpoints preserve the same executor and invariants.
18. One canonical IR feeds generated code, docs, descriptions, diagnostics, and semantic diffing.
19. All public docs/examples describe v2; old syntax remains only in migration material/tests.
20. `just verify` passes on MSRV and supported feature combinations.

## Final implementation report

When the work is finished, provide a concise but evidence-based report containing:

- the implemented v2 language and generated-usage summary;
- the canonical IR and responsibility-boundary summary;
- the runtime changes for deadline, retry, auth, rate, pagination, response terminals, redaction, and observation;
- the migration behavior and intentionally removed v1 surface;
- major files/modules changed;
- tests added by category;
- exact verification commands and results;
- benchmark/compile-time observations;
- any deliberate deviation from this prompt, including the property that forced it and evidence that the alternative is superior.

A deviation is not justified by convenience, v1 compatibility, or implementation difficulty alone.

# END IMPLEMENTATION PROMPT
