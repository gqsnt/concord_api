# Complete DSL reference

`concord::api!` compiles one remote HTTP contract into ordinary Rust client,
scope, endpoint, and request types. Rust types remain opaque to the macro.
Trait implementations determine body, response, provider, pagination, and
runtime behavior.

This page is the canonical owner for declaration grammar. Generated method
names and terminals are catalogued separately in the
[Generated API reference](generated-api.md).

## On this page

- [Top-level structure](#top-level-structure)
- [Generated namespaces and visibility](#generated-namespaces-and-visibility)
- [Named clauses and ordering](#named-clauses-and-ordering)
- [Feature-dependent capabilities](#feature-dependent-capabilities)
- [Values, optionality, and defaults](#values-optionality-and-defaults)
- [Explicit request bodies](#explicit-request-bodies)
- [Endpoint identity](#endpoint-identity)
- [Public expressions and request shaping](#public-expressions-and-request-shaping)
- [Authentication](#authentication)
- [Timeout](#timeout)
- [Pagination](#pagination)
- [Rate limits](#rate-limits)
- [Diagnostics](#diagnostics)

## Top-level structure

```rust
use concord::api;

api! {
    client ExampleApi {
        base "https://api.example.test/v1/";
        // client clauses in any order
    }

    scope users(tenant: String) {
        // scope clauses and children in any order

        GET get_user(id: String) -> Bytes {
            // endpoint clauses in any order
        }
    }
}
```

An invocation contains exactly one `client`, and that declaration must be
first. Root scopes and endpoints follow the client block. Declare a second
client in a separate `api!` invocation.

The structural grammar is:

```text
api        = client child*
client     = "client" IDENT "{" client_entry* "}"
child      = scope | endpoint
scope      = "scope" SNAKE_IDENT "(" parameters? ")" "{" scope_entry* "}"
endpoint   = METHOD SNAKE_IDENT "(" parameters? ")" "->" TYPE
             (";" | "{" endpoint_clause* "}")
parameters = parameter ("," parameter)* ","?
parameter  = ("#[body]")? IDENT ":" TYPE ("=" DEFAULT)?
```

The client name defines the generated client type. Scope and endpoint source
identifiers must be `snake_case` Rust identifiers. They directly define public
method and module names. Concord deterministically converts them to PascalCase
for generated types:

```text
scope match_v5  -> method/module match_v5, type MatchV5
GET match_by_id -> method match_by_id, endpoint type MatchById,
                   request type MatchByIdRequest
GET r#type      -> method r#type, endpoint type Type
```

Endpoint aliases are not supported. Write the desired public method name as the
endpoint identifier.

## Generated namespaces and visibility

Generated items follow the Rust module containing the `api!` invocation:

| Declaration | Generated location |
|---|---|
| `client CatalogApi` | public `CatalogApi` and `CatalogApiBuilder` beside the invocation |
| client module | public `catalog_api` beside the invocation |
| root endpoint `GET health` | `catalog_api::Health` and `catalog_api::HealthRequest` |
| `scope product_groups` | method/module `product_groups` and type `catalog_api::product_groups::ProductGroups` |
| endpoint `GET by_id` in that scope | `catalog_api::product_groups::ById` and `ByIdRequest` |

The generated items are public relative to their containing Rust module.
Ordinary Rust visibility of that containing module still controls whether
another crate can reach them. The DSL has no `pub`, `pub(crate)`, or
re-export clause; put the invocation in the module that should own the
surface, then use ordinary Rust re-exports if needed.

Scope modules nest according to declaration nesting. A child source identifier
therefore participates in both its Rust path and its default endpoint identity.
Each generated client module also contains the sealed generated `Endpoint`
contract used by `client.request(...)`; downstream crates cannot implement it.
There is no second public endpoint tree and no generated prelude.

## Named clauses and ordering

Named clauses are order-independent within their owner. These client
declarations are equivalent:

```rust
client First {
    base "https://example.test";
    vars { tenant: String; }
    timeout 5s;
}

client Second {
    timeout 5s;
    vars { tenant: String; }
    base "https://example.test";
}
```

Each scalar clause may occur at most once. A duplicate reports both the
duplicate and original spans. The supported clauses are:

| Owner | Clauses |
|---|---|
| client | `base`, `vars`, `credentials`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit` |
| scope | `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, plus child scopes/endpoints |
| endpoint | `id`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, `paginate` |

Order inside path atoms, query entries, header entries, authentication
placements, pagination fields, and rate-limit windows remains meaningful.
Several `auth` clauses form one ordered placement list even when other named
clauses occur between them. `auth replace` must be the first authentication
entry, and `auth off` is exclusive.

`base` is required but need not be textually first. It must be an absolute
`http` or `https` URL with a host and no username, password, query, or
fragment. A base path is allowed. The fixed host may be a DNS domain, an IPv4
literal, or an IPv6 literal.

## Feature-dependent capabilities

The macro grammar does not change by recognizing concrete helper names.
Cargo features make Rust types, implementations, transport capabilities, or
builder methods available:

| Façade feature | Declaration effect |
|---|---|
| `json` | exposes `Json<T>` as a request/response codec; there is no `json` DSL clause |
| `records` | exposes generic `Records<D>` streaming response support |
| `csv` / `ndjson` | expose concrete record decoders and also enable `records` |
| `multipart` | exposes the request-only `Multipart` body specification |
| `rate-limit` | installs the first-party local limiter by default; `rate_limit { ... }` grammar itself remains core |
| `oauth2-client` | exposes an OAuth2 provider type supplied through `credential ... = custom TYPE` |
| `default-tls`, `http2`, `gzip`, `brotli`, `deflate` | select managed transport capabilities outside declaration grammar |

Built-in offset, page-number, and cursor controllers require no optional
feature. Feature-disabled helper types fail through ordinary Rust name or
trait diagnostics. The proc macro does not inspect concrete type names to
infer a feature, implementation, or response mode. The normal
`.response_bytes()` request terminal requires no feature or DSL clause.

## Values, optionality, and defaults

Client values are declared in one `vars` block:

```rust
vars {
    tenant: String;
    locale: Option<String>;
    qualified: std::option::Option<String>;
    page_size: u32 = 25;
    label: Option<&'static str> = crate::DEFAULT_LABEL;
    tags: Option<Vec<&'static str>> = crate::DEFAULT_TAGS;
}
```

Scope and endpoint values use the same rules:

```rust
scope regions(region: String, locale: Option<String>) {
    GET item(id: u64, verbose: std::option::Option<bool>) -> Bytes;
}
```

Concord recognizes optionality syntactically, not semantically. Only
`Option<T>` and `std::option::Option<T>` (optionally with a leading `::`) are
absence-capable declarations. The generated `.set_name(...)` control accepts
`T`; for `Option<Vec<T>>`, it accepts `Vec<T>`. `.clear_name()` explicitly
selects `None`, which is omitted where the shaping operation permits it.

A type alias remains required because a proc macro cannot resolve aliases:

```rust
type MaybeLocale = Option<String>;

// `locale` is a required argument, not an optional set/clear control.
GET item(locale: MaybeLocale) -> Bytes;
```

Defaults use `name: Type = expression`. Defaulting is independent of absence:
an `Option<T>` or `Option<Vec<T>>` with a default has both capabilities and
therefore exposes `set_*`, `clear_*`, and `reset_*`. Accepted default
expressions are:

- a string literal;
- a signed base-10 integer literal;
- a boolean literal;
- a Rust const path.

Calls, methods, macros, blocks, and collection expressions are rejected.
Values with defaults are overridden with `.set_name(...)`. On a client
builder, `.clear_name()` records explicit absence while `.reset_name()`
discards the override so the declaration default is evaluated during
`build()`. The two operations remain observably different when the default is
`Some(...)`. On an already evaluated scope, explicit endpoint, or named request
snapshot, `.clear_name()` selects `None`, while `.reset_name()` restores the
retained default without re-evaluation.

Required client values use `.set_name(...)`. Required scope and endpoint
arguments preserve declaration order and remain constructor/navigation
arguments. Absence-capable and defaulted scope values independently gain
`clear_*` and `reset_*` alongside `set_*` on their scope and explicit endpoint
snapshots. Endpoint values use the same capability rule on explicit endpoint
snapshots and the named request wrapper. A wrapper does not gain inherited
scope controls it did not already own.

## Explicit request bodies

The request body role is always explicit:

```rust
POST update(
    id: String,
    #[body] payload: Json<UpdateInput>,
    revision: u64,
) -> Json<Item>;
```

`#[body]` is endpoint-only, accepts no arguments, and may appear at any
parameter position. The body name is not special. Exactly one body is allowed,
and it cannot have a default. A parameter literally named `body` is still an
ordinary value unless annotated; unannotated `body` is rejected to prevent
legacy name-based inference.

The body type must implement `RequestBodySpec`; the generated call accepts that
specification's input type. The response type after `->` must implement
`ResponseBodySpec`. Concord does not recognize `Json`, `Csv`, `NdJson`,
`Multipart`, `Stream`, or application type names in the compiler. Feature and
compatibility failures are ordinary trait-bound diagnostics at the declared
type.

`NoContent` is a response specification, not a request body specification.
A paginated endpoint may be bodyless or use a body specification that
implements the trusted `PageRequestBodySpec` contract. The macro remains
type-neutral and lets Rust enforce that obligation.

## Endpoint identity

An endpoint may declare a stable contract ID:

```rust
GET match_by_id(match_id: String) -> Json<Match> {
    id "riot.match_v5.match_by_id";
}
```

The ID is unique across all endpoints in one client and is validated as:

- 1–128 ASCII bytes total;
- dot-separated segments of 1–63 bytes;
- each segment begins and ends with a lowercase ASCII letter or digit;
- internal characters are lowercase ASCII letters, digits, `_`, or `-`.

An explicit ID remains stable when Rust client, scope, or endpoint names
change. If omitted, Concord derives:

```text
snake_case_client_name.scope_name.endpoint_name
```

Renaming any source component changes that default. Endpoint-local generated
rate-limit identity derives from the contract ID by appending `.rate_limit`.
Client and scope rate-limit identities derive from their deterministic source
paths. Use an explicit ID whenever telemetry or rate state must survive source
renames.

## Public expressions and request shaping

Public request-shaping expressions are deliberately restricted.

Accepted atoms are string literals, unsuffixed base-10 integer literals, and
direct references:

```rust
vars.name
scope.name
ep.name
```

Formatting concatenates one or more public atoms:

```rust
fmt["prefix-", ep.id, "-", scope.region]
```

Calls, indexing, methods, macros, chained fields, secrets, and credentials are
not public expressions. This prevents authentication material from entering
credential-sanitized URLs, public headers, descriptions, hooks, pagination
identity, or generated rate-limit keys.

### Host

```rust
host [expr, ...];
host off;
```

Host replaces inherited generated labels. Static labels are validated as DNS
labels. A non-empty final host-label list requires a DNS-domain fixed base;
host-prefix shaping cannot be applied to IPv4 or IPv6 literals. The compiler
checks this rule after client, scope, and endpoint inheritance is normalized.
IP-literal bases remain valid when the final list is empty, including when
`host off` removes an inherited prefix.

### Path

```rust
path [expr, ...];
path replace [expr, ...];
path off;
```

Normal path clauses append. Each expression becomes one encoded URL segment.

### Query

```rust
query {
    "name" = expr;
    remove "old";
}
query replace { ... }
query off;
```

Local keys are unique. Optional values append nothing; repeated values preserve
their iteration order.

### Headers

```rust
headers {
    "X-Name" = expr;
    remove "X-Old";
}
headers replace { ... }
headers off;
```

Names are validated case-insensitively. Transport-owned and authentication
headers are rejected.

### Inheritance

- host: replace or off;
- path: append, replace, or off;
- query and headers: modify, replace, or off, with removals;
- auth: modify, replace, or off;
- timeout: set, off, or inherit by omission;
- rate limits: additive lexical declarations.

Authentication query/header values are not public shaping. Core applies them
only at the private native boundary after safe observation and limiter
acquisition.

## Authentication

Credentials and secrets are client configuration:

```rust
credentials {
    secret api_key: String;
    secret token: String;
    secret user: String;
    secret password: String;

    credential keyed = api_key(secret.api_key);
    credential bearer_token = bearer(secret.token);
    credential login = basic(secret.user, secret.password);
    credential acquired = endpoint auth.login;
    credential provider = custom crate::Provider;
}
```

The supported source forms are:

- `api_key(secret.NAME)`;
- `bearer(secret.NAME)`;
- `basic(secret.USER, secret.PASSWORD)`;
- `endpoint ENDPOINT_PATH`;
- `custom RUST_TYPE`, where the type implements `CredentialProvider`.

The macro treats provider types as opaque and has no OAuth2-specific syntax.
Construct helpers such as `OAuth2ClientCredentials` in Rust and supply them
through the generated named client builder.

Place credentials lexically:

```rust
auth bearer bearer_token challenge unauthorized;
auth basic login challenge unauthorized_or_forbidden;
auth header "X-Key" = keyed challenge never;
auth query "token" = keyed;
```

Material types must match their placement. Public/authentication target
collisions fail during request preparation. Native credential values are never
available to the macro or semantic extensions.

Bearer and Basic are the only built-in placements that own `Authorization`.
Generic private-header placement supports ordinary custom names such as
`X-API-Key` but rejects `Authorization`; private authentication query
placement remains supported. Therefore complete custom `Authorization`
schemes such as `Signature`, `AWS4-HMAC-SHA256`, and `MAC` cannot be expressed
through the current semantic placement model. They must not be placed in
public headers.

The restriction preserves one private `Authorization` owner, collision
prevention, limiter-before-authentication ordering, and redaction. A future
dedicated authorization or request-signing contract is a separate design.

## Timeout

Retry is intentionally not declaration grammar. Install a managed
`RetryPolicy` with the generated `.retry(policy)` builder method or through
`configure_runtime`. See [Managed retries](../features/retries.md).

Timeouts are available at client, scope, and endpoint layers:

```rust
timeout 250ms;
timeout 5s;
timeout 10m;
timeout 1h;
timeout off;
```

Durations are positive base-10 integer literals with `ms`, `s`, `m`, or `h`.
Omission inherits the parent selection. Request-local APIs can override,
disable, or restore the generated selection.

## Pagination

```rust
paginate ControllerType {
    field: bind ep.destination,
    field: bind scope.destination,
    stable_field: initializer,
}
```

Any Rust controller type is accepted. The macro does not recognize built-in
controller names.

Live bindings accept only direct `ep.FIELD` or `scope.FIELD` destinations.
Fields and destinations are unique. They initialize from request state and are
copied into each fresh sequential page request. A `#[body]` parameter is not
an endpoint value and cannot be a pagination binding destination.

Stable initializers accept:

- string, integer, or boolean literals;
- direct `vars`, `scope`, or `ep` references;
- approved simple Rust paths/constants.

Shorthand, struct rest, calls, methods, closures, async blocks, arbitrary
blocks, secrets, credentials, and indirect bindings are rejected. Rust checks
field visibility, field types, controller trait bounds, and `Send`/`Sync`
requirements.

Pagination is available only to buffered, page-capable response
specifications. Its request is either bodyless or uses a
`PageRequestBodySpec`. That marker requires a cloneable input and promises
that every clone independently prepares the same page-stable body; ordinary
`Clone` or `RequestBodySpec` implementation is not sufficient. The macro does
not recognize concrete body type names.

The body remains unchanged while live endpoint/scope bindings advance. Body
bytes are excluded from cycle identity. Pagination produces the bounded,
sequential, cycle-detecting, all-or-error `.paginate(limit)` terminal.

## Rate limits

```rust
rate_limit {
    by [host, endpoint, vars.tenant, scope.region, ep.account];
    cost 2;
    100 / 1m;
    1_000 / 1h;
}
```

The exact clause grammar is:

```text
rate_limit {
    [by [DIMENSION, ...];]
    [cost POSITIVE;]
    WINDOW+
}
```

`by` and `cost` may occur in either order. Each may occur at most once, and
both must precede the first window. `by` must be nonempty, preserves source
order, rejects duplicate dimensions, and accepts at most
`MAX_RATE_LIMIT_KEY_PARTS` dimensions. Omitting `by` is the declaration-global
form: all requests to which that declaration applies share one bucket. There
is no `by global` spelling.

`cost` defaults to 1. It is positive and cannot exceed any window capacity. A
rule has at least one positive window. Window order is preserved.

At most one rule exists at each client, scope, or endpoint layer. Effective
plan order is client, outer scopes, inner scopes, then endpoint. Rules are
additive; descendants cannot replace or disable ancestors.

The declaration location and key dimensions have separate jobs:

```text
declaration location = where the rule applies
by [...]             = how applicable requests share buckets
```

Concord never infers dimensions. Exactly these dimension forms are supported:

| Dimension | Resolution |
|---|---|
| `host` | final logical host after all host shaping |
| `endpoint` | stable endpoint contract ID |
| `vars.name` | required or defaulted scalar client value |
| `scope.name` | required or defaulted scalar visible scope value |
| `ep.name` | required or defaulted scalar endpoint value |

Lexical availability is:

| Declaration owner | Available dimensions |
|---|---|
| client | `host`, `endpoint`, `vars.*` |
| scope | `host`, `endpoint`, `vars.*`, visible `scope.*` |
| endpoint | `host`, `endpoint`, `vars.*`, visible `scope.*`, `ep.*` |

References are resolved at the declaration site. An inherited client or scope
rule retains those resolved references and cannot begin using values available
only in a descendant. Ambiguous or unavailable scope references are rejected.

Value dimensions must be required or defaulted scalars. Optional values,
vectors, optional vectors, any value controlled by pagination, secrets,
credentials, formatted expressions, and literals are rejected. This
pagination check covers effective inherited and local rules at the final
endpoint: direct `vars.*`, `scope.*`, and `ep.*` dependencies are rejected.
`host` is rejected when final host shaping recursively depends on any
pagination-controlled value. Bare
`method`, `path`, `query`, `safe_url`, and `body` dimensions are unsupported,
as are unknown namespaces. `secret.*` and `credential.*` cannot enter a key.

Only value types actually referenced by `vars.*`, `scope.*`, or `ep.*` gain a
`Display` bound. Their `Display` output must be stable and canonical for local
quota identity. Formatting is captured incrementally into the remaining hard
aggregate key budget; the first excess fails with a redacted configuration
error. `host` and `endpoint` are already canonical strings and add no
application type bound.

Bucket identity is the selected limiter object, deterministic declaration
rule ID, ordered resolved dimensions, and window. Dimension order is
significant.
Concrete values may be operationally sensitive. They are supplied to the
selected limiter but do not appear in errors, `Debug`, `Display`, hooks,
completion, endpoint descriptions, generated Rustdoc, or safe request plans.
Generated endpoint Rustdoc lists the rule origin, deterministic declaration
rule ID, dimension names (or `declaration-global`), cost, and windows—never
resolved values.

A generated declaration rule ID is source-derived and deterministic within
one generated client contract. Renaming its client/scope/endpoint source
identity changes the ID. IDs are not globally unique across unrelated `api!`
expansions. Default-built clients own separate limiter objects, so unrelated
contracts do not share state by default. Reusing one limiter across instances
of the same generated client intentionally shares matching declaration state;
reusing one limiter across unrelated generated client contracts requires
their rule namespaces to be compatible with the application's intended
sharing.

Observer and concrete limiter selection are runtime policy:

```rust
let client = ExampleApi::builder()
    .configure_runtime(|runtime| {
        runtime.rate_limiter(my_limiter)
    })
    .build()?;
```

With the façade `rate-limit` feature, the façade composes a new
`LocalRateLimiter` into each built client. An explicitly configured limiter
takes precedence. Without either, non-empty generated plans fail closed.

## Diagnostics

The macro diagnoses structural contract mistakes and leaves semantic Rust type
mistakes to the compiler.

Structural diagnostics include:

- missing or repeated client declarations;
- missing/duplicate/invalid base URLs;
- non-snake scope or endpoint identifiers and duplicate child names;
- duplicate clauses with both spans;
- invalid or duplicate contract IDs;
- invalid, defaulted, or multiple body roles;
- invalid public expression namespaces and shaping literals;
- host-prefix shaping over an IPv4 or IPv6 fixed base;
- duplicate query, header, authentication, pagination, and rate-limit entries;

Rust diagnostics cover:

- missing body, response, provider, controller, or generated endpoint traits;
- wrong input/output/material types;
- feature-disabled types;
- inaccessible or incorrectly typed controller fields;
- unsupported endpoint capability methods;
- generated bridge version mismatch.

Diagnostics identify declarations, types, and safe structural categories. They
must not contain runtime values, secrets, credential material, protected URLs,
or native requests.

## Related

- [The contract model](../concepts/contract-model.md)
- [Understand generated Rust](../guide/generated-rust.md)
- [Generated API reference](generated-api.md)
- [Feature flags](features.md)
- [Unsupported forms](unsupported.md)
