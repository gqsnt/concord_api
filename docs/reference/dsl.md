# DSL reference

> Audience: all declaration authors
> Purpose: exact structural syntax, naming, identity, and diagnostics

`concord::api!` compiles one remote HTTP contract into ordinary Rust client,
scope, endpoint, and request types. Rust types remain opaque to the macro.
Trait implementations determine body, response, provider, pagination, and
runtime behavior.

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
| client | `base`, `vars`, `credentials`, `retry`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit` |
| scope | `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, plus child scopes/endpoints |
| endpoint | `id`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, `paginate` |

Order inside path atoms, query entries, header entries, authentication
placements, pagination fields, and rate-limit windows remains meaningful.
Several `auth` clauses form one ordered placement list even when other named
clauses occur between them. `auth replace` must be the first authentication
entry, and `auth off` is exclusive.

`base` is required but need not be textually first. It must be an absolute
`http` or `https` URL with a host and no username, password, query, or
fragment. A base path is allowed.

## Values, optionality, and defaults

Client values are declared in one `vars` block:

```rust
vars {
    tenant: String;
    locale: Option<String>;
    qualified: std::option::Option<String>;
    page_size: u32 = 25;
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
optional declarations. The generated setter accepts `T`, and an absent value
is omitted where the shaping operation permits it.

A type alias remains required because a proc macro cannot resolve aliases:

```rust
type MaybeLocale = Option<String>;

// `locale` is a required argument, not an optional setter.
GET item(locale: MaybeLocale) -> Bytes;
```

Defaults use `name: Type = expression`. Accepted expressions are:

- a string literal;
- a signed base-10 integer literal;
- a boolean literal;
- a Rust const path.

Calls, methods, macros, blocks, and collection expressions are rejected.
Default setters remain overridable; generated reset methods restore deferred
evaluation of the declaration default.

Required scope and endpoint arguments preserve declaration order. Optional and
defaulted values become inherent setters on the named request wrapper.

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
Pagination requires a bodyless endpoint.

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
labels.

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

## Retry and timeout

Client declarations support:

```rust
retry off;
retry reqwest_default;
```

Omission defaults to off. Custom transport retry is runtime policy and is not
valid DSL:

```rust
let client = ExampleApi::builder()
    .configure_http(|http| {
        http.dangerous_custom_retry(retry_policy())
    })
    .build()?;
```

That builder method requires the explicit `dangerous-custom-retry` feature.
Native transport retry remains inside one Concord attempt and is distinct from
the one-additional-attempt authentication recovery limit. It may perform
multiple physical wire sends inside one visible Concord attempt; those sends
share one limiter admission, hook attempt, and completion accounting unit.

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
copied into each fresh sequential page request.

Stable initializers accept:

- string, integer, or boolean literals;
- direct `vars`, `scope`, or `ep` references;
- approved simple Rust paths/constants.

Shorthand, struct rest, calls, methods, closures, async blocks, arbitrary
blocks, secrets, credentials, and indirect bindings are rejected. Rust checks
field visibility, field types, controller trait bounds, and `Send`/`Sync`
requirements.

Pagination is available only to bodyless, buffered, page-capable response
specifications. It produces the bounded, sequential, cycle-detecting,
all-or-error `.paginate(limit)` terminal.

## Rate limits

```rust
rate_limit {
    cost 2;
    100 / 1m;
    1000 / 1h;
}
```

`cost` is optional and defaults to 1. It is positive and cannot exceed any
window capacity. A rule has at least one positive window. Window order is
preserved.

At most one rule exists at each client, scope, or endpoint layer. Effective
plan order is client, outer scopes, inner scopes, then endpoint. Rules are
additive; descendants cannot replace or disable ancestors.

Observer and concrete limiter selection are runtime policy:

```rust
let client = ExampleApi::builder()
    .configure_runtime(|runtime| {
        runtime.rate_limiter(my_limiter)
    })
    .build()?;
```

With the façade governor feature, the façade composes the generated default
limiter. An explicitly configured limiter takes precedence. Without either,
non-empty generated plans fail closed.

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
