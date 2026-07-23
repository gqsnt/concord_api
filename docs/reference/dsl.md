# DSL reference

> Audience: All DSL users  
> Purpose: exact structural syntax and diagnostics

This is the consolidated reference for the `api!` declaration language. Rust types retain their normal meaning; the macro owns structure, inheritance, and focused structural diagnostics.

## DSL reference

`concord_macros::api!` accepts exactly one client declaration. The client is required, must be first, and contains configuration only. Root scopes and endpoints follow the client block.

### Top-level grammar

```text
api      = client child*
client   = "client" IDENT "{" client_config "}"
child    = scope | endpoint
vars     = "vars" "{" variable* "}"
variable = IDENT ":" TYPE ("=" DEFAULT)? ";"
```

The client begins with a fixed `base` URL and follows the supported configuration order. Use separate `api!` invocations when a module needs multiple generated clients.

Reference pages:

- [Client declarations](dsl.md#dsl-client-declarations)
- [Variables and secrets](dsl.md#dsl-variables-and-secrets)
- [Scopes and endpoints](dsl.md#dsl-scopes-and-endpoints)
- [Expressions](dsl.md#dsl-expressions)
- [Request shaping](dsl.md#dsl-request-shaping)
- [Bodies and responses](dsl.md#dsl-bodies-and-responses)
- [Authentication](dsl.md#dsl-authentication)
- [Retry and timeout](dsl.md#dsl-retry-and-timeout)
- [Pagination](dsl.md#dsl-pagination)
- [Rate limits](dsl.md#dsl-rate-limits)
- [Diagnostics](dsl.md#dsl-diagnostics)

### Layer clause order

For client, scope, and endpoint layers:

```text
host
path
query
headers
auth
timeout
rate_limit
```

Pagination follows endpoint layer clauses. Client variables, credentials, and retry occur before layer clauses. Client `rate_limit_observer` follows the client rate-limit declaration and precedes the end of the client block. Root children are parsed only after that block closes.

The macro intentionally rejects arbitrary Rust expressions and semantic type-name dispatch.

## DSL: client declarations

```rust
api! {
    client NAME {
        base "https://example.com/root/";
        vars { ... }
        credentials { ... }
        retry ...;
        host ...;
        path ...;
        query ...
        headers ...
        auth ...;
        timeout ...;
        rate_limit { ... }
        rate_limit_observer PATH;
    }

    scope ...
    ENDPOINT ...
}
```

### `base`

Required, first, and unique. It must be an absolute `http` or `https` URL with host and without username, password, query, or fragment. A base path is allowed.

### Client values

At most one `vars { ... }` block follows the base. Entries inside it are direct declarations without a `var` keyword. See [Variables and secrets](dsl.md#dsl-variables-and-secrets).

### Credentials

At most one credentials block follows client values.

### Retry

Client-only. Defaults to off when omitted.

### Layer and observer

Client request shaping, auth, timeout, and one lexical rate-limit rule follow in fixed order. `rate_limit_observer` is client-only and unique.

### Children

Root scopes and endpoints follow the closing client brace. Although they are syntactic siblings of `client`, they belong to that single generated client, inherit its configuration, and must have unique generated snake-case names within their owner.

### Multiple clients

Declare each generated client in an independent `api!` invocation. Distinct invocations may coexist in the same module and produce distinct endpoint capabilities and rate-limit declaration identities.

## DSL: variables and secrets

### Client variables

```rust
vars {
    tenant: String;
    locale: Option<String>;
    page_size: u32 = 25;
}
```

A variable can be:

- required;
- optional (`Option<T>`);
- defaulted.

Defaults accept only:

- string literal;
- signed integer literal;
- boolean literal;
- Rust const path.

Calls, methods, macros, blocks, and collection expressions are rejected.

Variables are immutable client snapshots and are referenced as `vars.name` in public expressions and stable pagination initializers.

Defaulted variables are omitted from the convenience constructor. They remain overridable with builder setters, and their generated reset methods restore deferred evaluation of the declared default.

### Scope and endpoint parameters

```rust
scope Regions(region: String, locale: Option<String>) { ... }
GET Item(id: u64, verbose: Option<bool>) -> Bytes;
```

The final endpoint parameter may be `body: BodySpec`.

### Secrets

Secrets exist only inside `credentials`:

```rust
secret api_key: String;
```

Concord 0.1 secrets are required `String` values without defaults. They are referenced only as `secret.name` inside credential source declarations.

Secrets cannot be used in host, path, query, headers, pagination stable fields, or rate-limit declarations.

### Naming

Names must be unique within their relevant owner. References are resolved statically. Optional values are omitted from request shaping when absent.

## DSL: scopes and endpoints

### Scope

```rust
scope NAME(parameter: Type, ...) {
    layer configuration
    nested scopes and endpoints
}
```

Scopes may nest to depth 64. Retry, the `vars` block, base, credentials, and rate-limit observers are client-only.

### Endpoint

```rust
METHOD NAME(parameters...) -> ResponseType;
```

or:

```rust
METHOD NAME(parameters...) as alias -> ResponseType {
    layer configuration
    paginate ...
}
```

Every endpoint requires parentheses. The body parameter, when present, must be final and named `body`.

### Methods

Standard verbs are recognized directly. Extension method:

```rust
METHOD("PROPFIND") Resource() -> Bytes;
```

Method token must be valid HTTP syntax.

### Aliases

`as alias` selects the public generated method name.

### Generated collision rules

Within one owner, scope and endpoint child names must remain unique after deterministic snake-case normalization. Reserved generated methods are rejected.

### Endpoint capabilities

Rust trait selection determines body/response compatibility, metadata terminal, pagination, and dangerous raw terminal. The macro does not infer custom type behavior from names.

## DSL: expressions

Public request-shaping expressions are deliberately restricted.

### Literals

- string literal;
- unsuffixed base-10 integer literal.

### References

```rust
vars.name
scope.name
ep.name
```

Only direct one-segment references are accepted. Calls, indexing, methods, macros, and chained fields are rejected.

### Formatting

```rust
fmt["prefix-", ep.id, "-", scope.region]
```

`fmt[...]` concatenates one or more supported public expressions. Nesting is bounded to 64. Trailing comma is not accepted inside `fmt`.

### Optional and repeated values

Optional values are supported in query/header/value positions according to generated type behavior. Repeated query values accept iterable endpoint values and preserve order.

### Security

Secrets and credentials have no public-expression namespace. This prevents them from accidentally entering logical URLs, public headers, pagination identity, hooks, or generated rate-limit plans.

### Display bounds

A dynamic value used in host/path/query/header formatting must implement the required formatting trait. Merely participating in lexical rate limiting does not add a `Display` bound.

## DSL: request shaping

### Host

```rust
host [expr, ...];
host off;
```

Host replaces inherited generated labels. Static labels are macro-validated.

### Path

```rust
path [expr, ...];
path replace [expr, ...];
path off;
```

Normal path appends. Each expression becomes one encoded segment.

### Query

```rust
query {
    "name" = expr;
    remove "old";
}
query replace { ... }
query off;
```

Local keys are unique. Optional values append nothing; repeated values preserve order.

### Headers

```rust
headers {
    "X-Name" = expr;
    remove "X-Old";
}
headers replace { ... }
headers off;
```

Names are case-insensitive and validated. Reserved headers are rejected.

### Inheritance

- Host: replace/off.
- Path: append/replace/off.
- Query: modify/replace/off with remove.
- Headers: modify/replace/off with remove.
- Auth: modify/replace/off.
- Timeout: set/off/inherit by omission.
- Rate limit: additive lexical declaration only.

### Authentication separation

Auth query/header values are not part of public query/header shaping. They are materialized only at the native boundary.

## DSL: bodies and responses

### Body parameter

```rust
POST Create(body: Json<CreateInput>) -> Json<Item>;
```

The body parameter must be final and named `body`. Its Rust type must implement `RequestBodySpec`; the generated method argument type is `B::Input`.

Built-ins:

```rust
Json<T>
Text
Bytes
Stream<M>
Multipart
application-defined RequestBodySpec
```

`NoContent` is not a request body specification.

### Response type

The type after `->` must implement `ResponseBodySpec`.

Built-ins:

```rust
Json<T>
Text
Bytes
NoContent
Stream<M>
Records<D>
application-defined ResponseBodySpec
```

### Type neutrality

The macro stores the body as `Option<syn::Type>` and response as `syn::Type`. It does not recognize built-in or application type names semantically. Feature availability and compatibility are checked by Rust traits.

### Pagination

A page-capable response must implement `PageResponseSpec`, use buffered acquisition, and produce an `IntoIterator` output. Bytes, discard, streams, and records are not paginated automatically.

### Features

Feature-disabled types are unavailable and produce focused Rust/macro diagnostics. The macro does not silently substitute a different mode.

## DSL: authentication

### Credentials block

```rust
credentials {
    secret key: String;
    credential api = api_key(secret.key);
    credential bearer_token = bearer(secret.token);
    credential basic_login = basic(secret.user, secret.password);
    credential acquired = endpoint Login;
    credential provider = custom crate::Provider;
}
```

Sources:

- `api_key(secret.NAME)`
- `bearer(secret.NAME)`
- `basic(secret.USER, secret.PASSWORD)`
- `endpoint EndpointName`
- `custom RustTypePath`, where the type implements `CredentialProvider`

The custom type names the provider, not its material. Generated storage and
authentication placement use `<Provider as CredentialProvider>::Material`.
The macro does not recognize OAuth2 or any provider implementation name.

### Placement

```rust
auth bearer credential;
auth basic credential;
auth header "X-Key" = credential;
auth query "token" = credential;
```

Optional challenge:

```rust
challenge unauthorized
challenge unauthorized_or_forbidden
challenge never
```

### Composition

Several `auth` statements can appear contiguously in a layer.

```rust
auth replace bearer token;
auth header "X-Key" = key;
```

At most one `replace`, and it precedes local placements. `auth off;` is exclusive.

### Validation

Credential material type must match placement: Basic requires `BasicCredential`; bearer/header/query require secret credential material where appropriate. Public header/query collisions and duplicate auth targets fail before send.

## DSL: retry and timeout

### Retry

Client-only:

```rust
retry off;
retry reqwest_default;
retry custom crate::retry_factory;
```

Default when omitted: off.

`retry custom` requires `dangerous-custom-retry` and a path returning the native retry builder.

Native retry stays inside one Concord attempt and is separate from authentication recovery.

### Timeout

Layer syntax:

```rust
timeout 250ms;
timeout 5s;
timeout 10m;
timeout 1h;
timeout off;
```

Durations are positive base-10 integer literals with `ms`, `s`, `m`, or `h`. Overflow is rejected.

A request-local override can set, disable, or inherit the generated timeout.

### Scope

Retry cannot appear in a scope or endpoint. Timeout can be inherited and changed at client, scope, and endpoint layers.

## DSL: pagination

```rust
paginate ControllerType {
    field: bind ep.destination,
    field: bind scope.destination,
    stable_field: initializer,
}
```

### Controller type

Any Rust type is accepted. The macro does not recognize offset, page, cursor, or custom semantics.

### Live bindings

`bind` is accepted only for direct `ep.FIELD` or `scope.FIELD` destinations. Fields and destinations must be unique.

Bound fields:

- initialize from request-local state once;
- are copied into every fresh page endpoint clone.

### Stable initializers

Accepted forms:

- string/integer/boolean literal;
- direct `vars`, `scope`, or `ep` value;
- approved simple Rust path/const.

No shorthand, rest syntax, call, method, closure, async block, arbitrary block, secret, credential, or indirect binding.

### Rust checks

Ordinary Rust reports missing/unknown/inaccessible fields, type mismatches, non-controller types, and non-`Send` controllers.

### Endpoint restriction

Pagination requires a bodyless endpoint and a page-capable buffered response. It generates a directly awaitable `.paginate(limit)` terminal.

## DSL: rate limits

### Rule

```rust
rate_limit {
    cost 2;
    100 / 1m;
    1000 / 1h;
}
```

`cost` is optional and defaults to 1. It must be positive and cannot exceed any window capacity. At least one positive window is required.

Durations use `ms`, `s`, `m`, or `h`.

### Placement

At most one rule per client, scope, or endpoint layer.

Effective plan order:

```text
client
outer scopes
inner scopes
endpoint
```

Rules add; descendants cannot replace, disable, or modify ancestors.

### Observer

Client-only and unique:

```rust
rate_limit_observer crate::Observer;
```

### Generated semantics

Each declaration uses a deterministic identity and global key. Runtime values, host, method, endpoint arguments, and pagination continuation do not enter generated keys.

Use prepared explicit `RateLimitRule` or a custom limiter for advanced keyed policy.

## DSL: diagnostics

The macro aims to diagnose structural mistakes at the declaration and leave normal type mistakes to Rust.

### Structural diagnostics

Examples:

- missing, misplaced, or second client declarations and duplicate bases;
- invalid fixed URL;
- duplicate normalized generated names;
- wrong clause order;
- client-only declaration in scope/endpoint;
- unsupported expression namespace;
- invalid static host/path/header;
- duplicate query/header/auth targets;
- invalid credentials source shape;
- invalid pagination field/binding grammar;
- invalid lexical rate-limit cost/window;
- unsupported structural syntax with a focused diagnostic.

### Rust diagnostics

Rust reports:

- missing body/response trait;
- wrong input/output type;
- feature-disabled type;
- inaccessible/missing controller field;
- non-`Send` controller/input/output;
- unsupported endpoint capability method;
- core/macro generated contract mismatch.

### Value safety

Diagnostics should identify declarations, types, and categories, not runtime values. Secret and credential references are structural names only.

### Trybuild

The repository uses Trybuild for source-only compile-fail boundaries. Standalone Cargo fixtures are reserved for dependency graph, feature absence, renamed dependency, and package metadata cases.
