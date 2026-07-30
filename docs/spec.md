# Concord DSL specification

`concord::api!` compiles one HTTP contract into a typed Rust client with
scope, endpoint, and request types. Rust types are opaque to the macro and
participate through trait implementations.

Unknown keywords are rejected at parse time.

## Structure

```text
api        = client child*
client     = "client" IDENT "{" client_entry* "}"
child      = scope | endpoint
scope      = "scope" SNAKE_IDENT "(" parameters? ")" "{" scope_entry* "}"
endpoint   = METHOD SNAKE_IDENT "(" parameters? ")" "->" TYPE
             (";" | "{" endpoint_clause* "}")
parameters = parameter ("," parameter)* ","?
parameter  = ("#[body]")? IDENT ":" TYPE ("=" DEFAULT)?
DEFAULT    = "default" | STRING | SIGNED_INTEGER | BOOLEAN | CONST_PATH
```

An invocation contains exactly one `client`, and the client is first. Root
scopes and endpoint leaves follow the client block. A second client requires a
second invocation.

Scope and endpoint identifiers are `snake_case` Rust identifiers. The
identifier defines the generated method and module name. Concord derives
PascalCase endpoint and request type names from it.

The containing Rust module controls visibility. The DSL has no visibility or
re-export clause.

## Clauses

Named clauses are order-independent within one owner. Each scalar clause
occurs at most once.

| Owner | Clauses |
|---|---|
| client | `base`, `vars`, `credentials`, `rate_limits`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit` |
| scope | `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, child scopes, endpoint leaves |
| endpoint leaf | `id`, `host`, `path`, `query`, `headers`, `auth`, `timeout`, `rate_limit`, `paginate` |

Order is significant inside path atoms, query entries, header entries,
authentication placements, pagination fields, rate-limit attachments, key
dimensions, and windows.

## Fixed base

```concord
client ExampleApi {
    base "https://api.example.test/v1/";
}
```

`base` is required. It is an absolute `http` or `https` URL with a host. It
contains no username, password, query, or fragment. Its path may be nonempty.
Its host may be a DNS domain, IPv4 literal, or IPv6 literal.

## Values and defaults

Client values are declared in one `vars` block:

```concord
vars {
    tenant: String;
    locale: Option<String>;
    page_size: u32 = 25;
    tags: Vec<String> = default;
    label: Option<&'static str> = crate::DEFAULT_LABEL;
}
```

Scope and endpoint values use the same `name: Type` and
`name: Type = DEFAULT` forms in their parameter lists.

Only `Option<T>` and `std::option::Option<T>`, with an optional leading `::`,
are absence-capable. A type alias that resolves to `Option<T>` is not
absence-capable to the macro.

Accepted defaults are the `default` shorthand, string literals, signed
base-10 integer literals, boolean literals, and Rust const paths. Calls,
methods, macros, blocks, and collection expressions are rejected. Rust checks
the declared type for `Default` when the shorthand is used.

Absence and default capabilities are independent. An absence-capable value
with a default has `set_*`, `clear_*`, and `reset_*` controls. For
`Option<T>`, `set_*` accepts `T`.

Client defaults are evaluated by each build that has no override. Scope
defaults are evaluated when a scope snapshot is created. Endpoint defaults are
evaluated when an endpoint snapshot is created. Builder `reset_*` removes an
override; snapshot and request `reset_*` restore the retained evaluated
default.

Required client values use builder setters. Required scope and endpoint values
are arguments in declaration order.

## Request and response specifications

An endpoint request body uses exactly one `#[body]` parameter:

```concord
POST update(
    id: String,
    #[body] payload: Json<UpdateInput>,
) -> Json<Item>;
```

`#[body]` is endpoint-only, accepts no arguments, and may appear at any
parameter position. A request body has no declaration default. A parameter
without `#[body]` is an ordinary public value regardless of its name.

The body type implements `RequestBodySpec`; its associated input type becomes
the endpoint argument type. The response type implements `ResponseBodySpec`.
The macro does not recognize concrete body or response type names.

A paginated request is bodyless or uses a body specification that implements
`PageRequestBodySpec`. The marker requires a cloneable input and promises that
each clone prepares an independent page-stable body.

## Endpoint identity

```concord
GET by_id(id: String) -> Json<Item> {
    id "catalog.item.by-id";
}
```

An explicit endpoint ID is unique within the client. It is 1–128 ASCII bytes
in dot-separated segments of 1–63 bytes. Each segment begins and ends with a
lowercase ASCII letter or digit and contains only lowercase ASCII letters,
digits, `_`, or `-`.

Without `id`, Concord derives
`snake_case_client_name.scope_name.endpoint_name`. Renaming a source component
changes a derived ID.

## Public expressions

Public request-shaping atoms are string literals, unsuffixed base-10 integer
literals, `vars.name`, visible `scope.name`, and `ep.name`.

```concord
fmt["item-", ep.id, "-", scope.region]
```

`fmt[...]` concatenates one or more public atoms. Calls, indexing, methods,
macros, chained fields, secrets, and credentials are rejected.

`vars.*` is visible at every attachment site. `scope.*` resolves one visible
scope value and rejects ambiguous or unavailable names. `ep.*` is visible only
at its endpoint leaf.

## Host, path, query, and headers

```concord
host [scope.region, "api"];
path ["v1", "items", ep.id];
query {
    "locale" = vars.locale;
    "tag" = ep.tags;
}
headers {
    "X-Region" = scope.region;
}
```

`host [expr, ...]` replaces inherited generated host labels. `host off`
removes them. Each static or dynamic label is a 1–63 byte ASCII DNS label that
begins and ends with an alphanumeric character and contains only ASCII
alphanumerics or `-`. A final generated label list requires a DNS-domain base.
The complete host is at most 253 bytes.

`path [expr, ...]` appends atoms. `path replace [...]` replaces inherited
generated atoms. `path off` removes them. Each atom becomes exactly one
percent-encoded URL segment. Static and dynamic atoms reject the empty string,
`.`, `..`, `/`, and `\`.

`query { ... }` modifies inherited query entries. `query replace { ... }`
replaces them. `query off` removes them. `"name" = expr;` appends a
percent-encoded pair. An absent optional value appends nothing. A vector
appends one pair per element in iteration order. `remove "name";` removes the
inherited name. Local names are unique.

`headers { ... }` modifies inherited public headers.
`headers replace { ... }` replaces them. `headers off` removes them.
`"Name" = expr;` assigns one formatted value and `remove "Name";` removes the
inherited name. Names are case-insensitive. Transport-owned, cookie, proxy,
and authentication headers are rejected.

Public expressions may contain application data. They never contain DSL
secrets or credential material.

## Merge and precedence

| Construct | Operation |
|---|---|
| host | replace or off |
| path | append, replace, or off |
| query | modify, replace, or off |
| headers | modify, replace, or off |
| authentication | ordered modify, replace, or off |
| timeout | set, off, or inherit by omission |
| rate limits | additive |

Merge order is client, outer scope, inner scope, endpoint leaf. Local ordered
entries retain source order.

## Authentication

Client authentication configuration defines secrets and credentials:

```concord
credentials {
    secret token: String;
    secret user: String;
    secret password: String;

    credential api_token = api_key(secret.token);
    credential bearer_token = bearer(secret.token);
    credential login = basic(secret.user, secret.password);
}
```

Secret declarations are required `String` inputs without defaults.

Credential sources are:

- `api_key(secret.NAME)`;
- `bearer(secret.NAME)`;
- `basic(secret.USER, secret.PASSWORD)`;
- `endpoint ENDPOINT_PATH`;
- `custom RUST_TYPE`.

An endpoint-backed source names an endpoint leaf in the same client. A custom
source type implements `CredentialProvider`.

Credential placements are ordered:

```concord
GET bearer_auth() -> NoContent {
    auth bearer bearer_token challenge unauthorized;
}
GET basic_auth() -> NoContent {
    auth basic login challenge unauthorized_or_forbidden;
}
GET exact_auth() -> NoContent {
    auth authorization = bearer_token challenge never;
}
GET scheme_auth() -> NoContent {
    auth authorization "Token" = bearer_token challenge unauthorized;
}
GET header_auth() -> NoContent {
    auth header "X-API-Key" = api_token challenge never;
}
GET query_auth() -> NoContent {
    auth query "token" = api_token;
}
```

`auth replace` is the first authentication entry at its attachment site and
replaces inherited placements. `auth off` is exclusive and removes inherited
placements.

Bearer, Basic, exact Authorization, and static-scheme Authorization are
mutually exclusive owners of the private `Authorization` header. Generic
private-header placement rejects `Authorization`. Placement header names are
case-insensitive, and duplicate effective targets are rejected.

`auth authorization = CREDENTIAL` uses the credential as the complete header
value. `auth authorization "SCHEME" = CREDENTIAL` inserts the declared ASCII
HTTP-token scheme, one ASCII space, and the credential. A scheme is at most
256 bytes.

Static and endpoint sources default to `challenge never`. A custom provider
requires an explicit challenge. One operation performs at most one admitted
authentication recovery.

Secrets and credentials are not public expressions. They do not enter
logical URLs, public headers, safe plans, hooks, rate-limit keys, completion,
generated Rustdoc, or diagnostics. The runtime places credential material
after rate-limit admission at the private native request boundary.

Application client-certificate material is runtime HTTP configuration through
`identity_pem`; it is not an authentication attachment.

## Timeout

```concord
GET milliseconds() -> NoContent { timeout 250ms; }
GET seconds() -> NoContent { timeout 5s; }
GET minutes() -> NoContent { timeout 10m; }
GET hours() -> NoContent { timeout 1h; }
GET no_timeout() -> NoContent { timeout off; }
```

A duration is a positive base-10 integer followed by `ms`, `s`, `m`, or `h`.
Omission inherits the parent timeout.

Retry is runtime policy configured through the generated builder.

## Pagination

```concord
paginate concord::pagination::OffsetPagination {
    offset: bind ep.offset,
    page_size: bind ep.page_size,
}
```

The controller type is opaque to the macro. Rust checks controller fields,
types, visibility, and trait bounds.

A live binding is `bind ep.FIELD` or `bind scope.FIELD`. Fields and
destinations are unique. A request body is not a binding destination.

Stable initializers are string, integer, or boolean literals; direct
`vars.*`, `scope.*`, or `ep.*` references; or simple Rust paths and constants.
Calls, methods, closures, blocks, secrets, credentials, indirect bindings,
and the ordinary-value `default` shorthand are rejected.

Pagination requires a buffered page-capable response specification. It is
sequential, cycle-detecting, bounded, and all-or-error. The body remains stable
while bound values advance.

## Rate-limit profiles and policy

A rate-limit profile is a named reusable declaration in the client:

```concord
rate_limits {
    rule account_reads {
        id "service.account.read";
        by [host, account];
        cost 1;
        100 / 1m;
        1_000 / 1h;
    }
}
```

Attach it and bind each formal logical dimension:

```concord
rate_limit use account_reads {
    account = scope.account;
    cost 2;
}
```

A profile without formals uses `rate_limit use account_reads;`.

An anonymous attachment declares the same semantic policy inline:

```concord
rate_limit {
    id "service.item.read";
    by [host, endpoint, vars.tenant, scope.region, ep.item_id];
    cost 2;
    100 / 1m;
}
```

`rate_limits` is client-only and occurs at most once. Profile names are unique
snake-case identifiers. Every profile is attached at least once.

`id`, `by`, and `cost` may occur in any order before the first window. Each
occurs at most once. A policy contains at least one positive window; an empty
`rate_limit {}` is rejected. `cost` defaults to 1, is positive, and does not
exceed any window capacity.

`by` is nonempty, ordered, unique, and bounded. Omitting `by` creates a
declaration-global bucket for that attachment.

Profile dimensions use built-in `host` and `endpoint` or bare formal names.
Profiles do not capture `vars.*`, `scope.*`, or `ep.*`. Each attachment binds
every formal exactly once to a visible required or defaulted scalar reference.

Anonymous dimensions are `host`, `endpoint`, `vars.*`, visible `scope.*`, and
`ep.*` according to attachment-site visibility. Optional values, vectors,
pagination-controlled values, literals, formatted expressions, secrets, and
credentials are rejected.

`host` resolves the final logical host. `endpoint` resolves the endpoint ID.
Value dimensions require `Display`; their output is the policy key value.
Dimension order is significant.

Each attachment site accepts ordered named uses and at most one anonymous
attachment. Effective policy order is client, outer scope, inner scope, then
endpoint leaf. Descendants do not replace or disable inherited policies.

An explicit rate-limit ID is 1–256 ASCII bytes using the endpoint-ID segment
alphabet. Concord emits it exactly. Without an explicit ID, a profile derives
`<client>.rate_limit.<rule>` and an anonymous attachment derives a
source-based ID.

The same explicit ID may occur in disjoint branches of one client only when
its ordered dimension names and ordered windows match. The same ID does not
occur twice in one effective endpoint plan.

The resolved policy consists of the effective ID, ordered dimensions and
values, cost, and ordered windows. A selected limiter receives this policy.
Resolved dimension values do not enter errors, plans, hooks, completion,
descriptions, or generated Rustdoc.

## Diagnostics

The macro rejects structural contract violations. Rust rejects missing trait
implementations, incompatible associated types, inaccessible controller
fields, and feature-disabled helper types.

Diagnostics name declarations, violated constraints, and safe structural
categories. They do not contain runtime values, secret material, credential
material, protected URLs, request or response bodies, or native error chains.

## Conventions

- Put one `api!` invocation in the Rust module that owns its visibility.
- Use explicit endpoint IDs when source renames must not change identity.
- Use named rate-limit profiles when one policy is attached at multiple sites.
- Use public values only for data allowed in plans and hooks.
- Use `.test` hosts in compile-only examples.

## Glossary

- **client**: the generated immutable API root and its shared runtime snapshot.
- **scope**: an immutable selection that owns visible public values and child declarations.
- **endpoint leaf**: one HTTP operation declaration at the end of a scope path.
- **attachment site**: a client, scope, or endpoint leaf where inherited request semantics are added, replaced, or removed.
- **profile**: a named reusable declaration; v1 has rate-limit profiles.
- **policy**: resolved semantic data supplied to runtime enforcement.
- **public expression**: a restricted value expression allowed in request shaping.
- **logical URL**: the credential-free resolved URL used by public observation.
- **native request**: the private transport request after credential placement.

## Related

- [Runtime model](runtime.md)
- [Generated client](generated_client.md)
- [Authentication](auth.md)
- [Pagination](pagination.md)
- [Retry and rate limiting](retry_and_rate_limit.md)
