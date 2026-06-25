# DSL

Concord describes an HTTP API as a typed tree. The tree has a `client` root, optional `scope` branches, and endpoint leaves. The macro turns that tree into a facade-first Rust client and endpoint request plans.

## Shape

```rust
api! {
    client ExampleApi {
        base "https://api.example.com"
    }

    scope users {
        path ["users"]

        GET GetUser(id: u64)
            as get_user
            path [id]
            -> Json<User>
    }
}
```

The tree is the mental model to keep in mind:

- `client` defines the root type, base URL, root variables, credentials, grouped config, and reusable profiles.
- `scope` groups route fragments, host fragments, auth, behaviors, and policy attachments.
- An endpoint leaf defines one HTTP operation and its typed response.

## Client configuration

Client-level configuration is where reusable declarations live. Attachments happen later, at defaults, scope, or endpoint sites.

### Declarations and attachments

Use declarations to define reusable profiles:

- `secret token: String`
- `credential session = bearer(secret.token)`
- `retry read { ... }`
- `cache standard { ... }`
- `rate_limit app { ... }`
- `behavior read { ... }`

Use attachments to apply those profiles:

- `auth bearer session`
- `retry read`
- `cache standard`
- `rate_limit app`
- `behavior read`

Declarations belong in client-level config, usually grouped under `auth { ... }`, `policies { ... }`, or `behaviors { ... }`. Attachments belong in `default { ... }` / `defaults { ... }`, scopes, or endpoints. `policies { ... }` is for declarations and observers, not default attachments.

### Canonical client example

```rust
api! {
    client ExampleApi {
        base "https://api.example.com"

        auth {
            secret token: String
            credential api_token = bearer(secret.token)
        }

        policies {
            retry read {
                max_attempts 2
                methods [GET]
                on [429, 500, 502, 503, 504]
                retry_after
            }

            rate_limit app {
                bucket application by [host] {
                    100 / 1m
                }
            }

            observe rate_limit ExampleRateLimitHeaders
        }

        behaviors {
            behavior read {
                auth bearer api_token
                retry read
                rate_limit app
            }
        }

        defaults {
            behavior read
        }
    }

    GET Me
    path ["me"]
    -> Json<User>
}
```

A compile-checked version of the guide examples lives in `concord_examples/src/docs_dsl.rs`. Less common public syntax is compile-checked in `concord_examples/src/docs_advanced_dsl.rs`.

### Base URL

`base` declares the scheme and root domain.

```rust
client ExampleApi {
    base "https://api.example.com"
}
```

### Authentication

Secrets and credentials can be written directly in the client block or grouped under `auth { ... }`.

```rust
client ExampleApi {
    base "https://api.example.com"

    auth {
        secret token: String
        credential session = bearer(secret.token)
    }
}
```

This is equivalent to writing `secret` and `credential` directly in the client block. Auth use clauses such as `auth bearer session` do not belong in `auth { ... }`; they belong in defaults, scopes, endpoints, or behavior profiles.

### Policies

Retry, cache, rate-limit profile declarations, and policy observers can be grouped under `policies { ... }`.

```rust
client PolicyApi {
    base "https://example.com"

    policies {
        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500, 502, 503, 504]
            retry_after
        }

        cache standard {
            ttl 60s
            revalidate
            on_error serve_stale
        }

        rate_limit app {
            bucket application by [host] {
                100 / 1s
            }
        }

        observe rate_limit MyObserver
    }
}
```

Flat declarations still work when they are clearer. `policies { ... }` is the preferred grouped form for larger clients.

### Behaviors

Behavior profiles are semantic bundles over auth, cache, retry, and rate-limit policy.

```rust
client ExampleApi {
    base "https://api.example.com"

    behaviors {
        behavior read {
            auth bearer session
            retry read
            rate_limit app
        }
    }
}
```

Behavior profiles are a name layer over ordinary policy attachments. They do not change runtime semantics; they make repeated request behavior easier to read and move.

Merge rules:

- client defaults apply first
- scope policies apply from outer to inner
- endpoint policies apply last
- explicit `retry` and `cache` override behavior-provided `retry` and `cache` at the same attachment site
- behavior-provided `rate_limit` combines with explicit local `rate_limit`
- `rate_limit off` clears inherited rate-limit policy
- behavior rate-limit key bindings are resolved where the behavior is attached

Behavior profiles can also be grouped:

```rust
behaviors {
    behavior read {
        retry read
        rate_limit app
    }
}
```

This is equivalent to writing the behavior profiles directly in the client block. Flat behavior declarations still work.

A behavior can extend another behavior. Parent auth uses are inherited, parent rate-limit profiles are combined, and child `retry` / `cache` override parent `retry` / `cache`.

```rust
behavior read {
    retry read
    rate_limit app
}

behavior protected_read extends read {
    auth bearer session
}
```

Behavior `rate_limit` clauses are resolved where the behavior is attached. This lets a behavior carry the rate-limit profile while the endpoint supplies a contextual key binding.

```rust
rate_limit match_bucket {
    bucket method by [match_key] {
        5 / 1s
    }
}

behavior match_read {
    rate_limit match_bucket
}

GET Match(match_id: String)
path ["matches", match_id]
rate_limit key match_key = match_id
behavior match_read
-> Json<MatchDto>
```

Attaching the same behavior as a client default would fail because endpoint variables are not available at the client level.

### Defaults

Default attachments can be written with either `default { ... }` or `defaults { ... }`. The singular form is valid; the plural form is the preferred grouped form for larger clients.

```rust
defaults {
    behavior read
    retry read
    cache standard
    rate_limit app
    auth bearer session
}
```

`default` / `defaults` applies client-wide defaults before scope and endpoint attachments. A default behavior applies before explicit default `cache`, `retry`, and `rate_limit` clauses.

Use `rate_limit off`, `retry off`, or `cache off` on a narrower layer to clear inherited policy.

Only one default/defaults block is allowed per client.

## Scopes

Scopes shape the tree below the client root. They add route fragments, host fragments, and inherited attachments for their children.

```rust
scope users {
    path ["users"]
    behavior scope_read

    GET Me
    path ["me"]
    -> Json<User>
}
```

Scope-level attachments apply to all nested endpoints. Nested scopes inherit outer scopes, so scope policies flow from outer to inner before endpoint attachments are applied.

### Host and path

`host [...]` appends host labels before the base domain. `path [...]` appends path atoms.

```rust
scope tenant(tenant_id: String) {
    host [fmt["tenant-", tenant_id], "api"]
    path ["tenants", tenant_id]
}
```

`host [...]` is a scope-level route fragment. Use a scope when a group of endpoints needs dynamic or additional host labels.

Path atoms are encoded segment-by-segment. Split fixed path pieces into separate string atoms.

### Formatting with `fmt`

`fmt[...]` builds one wire atom from literals and variables.

```rust
path [fmt["org-", org_id]]
headers { "X-Trace" = fmt["trace-", vars.trace_id] }
query { "range" = fmt[start, "-", count] }
```

Ordinary route, query, header, timeout, and pagination expressions are public
request-shaping expressions. They may use endpoint arguments, declared client
variables in supported DSL reference positions, literals, and pure Rust
expressions that do not reference Concord generated internals. Outside those
resolved value forms, they cannot access `secret.*`, `auth.*`, generated locals
such as `cx`, `ep`, `vars`, `self`, or `request`, raw-identifier variants such
as `r#secret`, or secret exposure methods. Secrets belong in credential
declarations and explicit `auth` attachments, so raw auth material is inserted
only by auth materialization at transport send.

### Query

Shorthand uses the Rust argument name as both key and value.

```rust
query {
    count
}
```

Explicit keys use string literals.

```rust
query {
    "startTime" = start_time,
    "endTime" = end_time
}
```

Append repeated values with `+=`.

```rust
query {
    "tag" += primary_tag,
    "tag" += secondary_tag
}
```

Remove an inherited query key with `-`.

```rust
query {
    -"debug"
}
```

Optional argument values remove their query key when absent.

### Headers

Header keys are explicit string literals.

```rust
headers {
    "User-Agent" = "ExampleApi/1.0",
    "X-Trace" = fmt["trace-", vars.trace_id]
}
```

A narrower layer overrides inherited headers. Remove an inherited header with `-`.

```rust
headers {
    -"X-Trace"
}
```

## Endpoints

Endpoint leaves should be ordered as:

```rust
GET EndpointName(args...)
as optional_alias
path [...]
query { ... }
headers { ... }
paginate ...
behavior ...
cache/retry/rate_limit/auth ...
-> Json<Response>
```

Request bodies are endpoint signature arguments named `body`, for example `POST Create(body: Json<CreateUser>)`.

The response line should normally be the final line of the endpoint contract. Policy and behavior attachments come before the response line so the endpoint leaf stays visually closed by its return type.

Low-level policy details should be lifted into profiles or behaviors when they repeat.

### Arguments

Required arguments are direct facade method arguments.

```rust
GET GetUser(id: u64)
path ["users", id]
-> Json<User>
```

Optional arguments use `?` and default to absent.

```rust
GET Search(q?: String)
path ["search"]
query { q }
-> Json<Vec<User>>
```

Defaulted arguments use `=`.

```rust
GET List(start: u64 = 0, count: u64 = 20)
path ["items"]
query { start, count }
-> Json<Vec<Item>>
```

Optional arguments may also have defaults. They initialize as `Some(default)` and can still be cleared.

```rust
GET Search(region?: String = "euw1".to_string())
path ["search"]
query { region }
-> Json<SearchResult>
```

### Bodies

Bodies are endpoint signature arguments. The argument name must be `body`, and the codec wraps the Rust body type.

```rust
POST Create(body: Json<CreateItem>)
path ["items"]
-> Json<Item>
```

### Response mapping

Response mapping is the exception when used:

```rust
GET Login
path ["login"]
-> Json<LoginResponse>
map AccessToken { AccessToken::new(r.access_token) }
```

Behavior names also appear on endpoint docs. Generated endpoint documentation includes attached behavior names from client defaults, scopes, and endpoints. For example, a generated line can read: Behavior: `read`, `match_read`.

## Pagination

Pagination is declared on endpoints with a controller and controller field assignments.

```rust
GET ListItems(start: u64 = 0, count: u64 = 20)
path ["items"]
query { start, count }
paginate OffsetLimitPagination {
    offset = start,
    limit = count
}
-> Json<Vec<Item>>
```

Cursor pagination can use an optional cursor argument.

```rust
GET ListCursor(cursor?: String, count: u64 = 20)
path ["cursor-items"]
query { cursor, count }
paginate CursorPagination {
    cursor = cursor,
    per_page = count
}
-> Json<CursorPage>
```

Pagination remains an endpoint concern. It is not part of grouped policy or behavior declarations.

## Generated documentation

Generated endpoint documentation is derived from the resolved semantic model, not from raw syntax. That is why behavior names remain visible in rustdoc even though behavior semantics are lowered into ordinary auth, cache, retry, and rate-limit data.

## Keyword reference

This section is the compact v1 reference for public DSL syntax. Focused guides such as `docs/auth.md`, `docs/cache_retry_rate_limit.md`, `docs/pagination.md`, and `docs/customization.md` expand on these forms.

### Tree and routing

- `api! { ... }` wraps one API tree.
- `client Name { ... }` declares the generated client root.
- `base "https://api.example.com"` declares the scheme and root domain.
- `scope name(args...) { ... }` groups route fragments and inherited attachments.
- `host [...]` appends scope-level host labels before the base domain.
- `path [...]` appends encoded path segments.
- Endpoint methods use HTTP method identifiers such as `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS`.

`host` is scope-level v1 syntax. `host` and `path` atoms may be string literals, variables, or `fmt[...]` atoms.

### Client configuration

Preferred large-client grouping:

- `auth { ... }` contains `secret` and `credential` declarations.
- `policies { ... }` contains `retry`, `cache`, and `rate_limit` profile declarations plus `observe rate_limit`.
- `behaviors { ... }` contains `behavior` declarations.
- `defaults { ... }` contains client-wide attachments.

Compatibility forms remain valid:

- `secret`, `credential`, `retry`, `cache`, `rate_limit`, and `behavior` declarations may be written directly in `client`.
- `default { ... }` is accepted as an alias for `defaults { ... }`.

### Variables and arguments

- `var name: Type` declares a client variable available as `vars.name`.
- `secret name: Type` declares a redacted client input available to credential declarations as `secret.name`.
- Scope and endpoint arguments are written in the signature: `scope users(region: String)` or `GET User(id: u64)`.
- Optional arguments use `?`, for example `cursor?: String`.
- Defaulted arguments use `=`, for example `count: u64 = 20`.
- Optional defaulted arguments are supported, for example `region?: String = "euw1".to_string()`.
- Request bodies are endpoint arguments named `body`, for example `POST Create(body: Json<CreateItem>)`.

### Auth

Credential declarations:

```rust
auth {
    secret token: String
    secret username: String
    secret password: String
    secret client_id: String
    secret client_secret: String

    credential api = api_key(secret.token)
    credential session = bearer(secret.token)
    credential login = basic(secret.username, secret.password)
    credential oauth = oauth2_client {
        token_url: "https://auth.example.com/oauth/token",
        client_id: secret.client_id,
        client_secret: secret.client_secret,
        scope: "read",
    }
    credential acquired = endpoint auth_api::Login
}
```

Auth attachments:

```rust
auth bearer session
auth header "X-Api-Key" = api
auth query "api_key" = api
auth basic login
auth certificate client_cert
```

`auth certificate` is an attachment form for client-certificate credential material. The DSL does not provide a `certificate(secret...)` constructor in v1; use endpoint-backed or runtime-provided credential material when certificate auth is needed.

Default protected-request refresh behavior is intentionally conservative: `401 Unauthorized` and `403 Forbidden` both invalidate and retry after refresh by default. Refresh attempts are bounded. See `docs/auth.md` for the full auth runtime behavior.

Secret-bearing auth values are redacted from debug/display output and diagnostics. The actual outbound request still carries the required credential material.

### Retry

Retry profiles may be declared flat or inside `policies { ... }`.

```rust
retry write {
    max_attempts 3
    methods [POST, PUT]
    on [409, 429, 500]
    on transport [Timeout, Connect]
    retry_after
    idempotency header "Idempotency-Key"
}
```

Supported retry fields:

- `max_attempts N`
- `methods [GET, POST, ...]`
- `on [429, 500, ...]`
- `on transport [Timeout, Connect, Tls, Dns, Io, Request, Other]`
- `retry_after`
- `idempotency header "Header-Name"`

Attachments:

```rust
retry read
retry off
retry {
    max_attempts 1
}
```

Retry profiles may use `extends parent`.

### Cache

Cache profiles may be declared flat or inside `policies { ... }`.

```rust
cache standard {
    http
    ttl 60s
    revalidate
    on_error serve_stale
    capacity 10_000 entries
    max_body 2 mib
    shared
}
```

Supported cache profile fields:

- `http`
- `ttl 60s`, `ttl 1m`, or spelled duration units such as `ttl 1 minute`
- `revalidate`
- `on_error ignore`
- `on_error serve_stale`
- `capacity N entries`
- `max_body N bytes|kb|kib|mb|mib|gb|gib`
- `shared`

Attachments and shorthand forms:

```rust
cache standard
cache only standard
cache off
cache http
cache 5m
cache revalidate
cache stale_on_error
cache {
    max_body 128 kib
}
```

`cache stale_on_error` is shorthand for a local patch whose `on_error` behavior serves stale data. Cache profiles may use `extends parent`.

Cache sizing fields are runtime-backed:

- `capacity N entries` limits the maximum number of cache entries.
- `max_body N unit` limits the cached response body size.
- `shared` enables shared cache mode.

Size units are decimal for `kb`, `mb`, and `gb`, and binary for `kib`, `mib`, and `gib`:

- `bytes = 1`
- `kb = 1_000`
- `kib = 1_024`
- `mb = 1_000_000`
- `mib = 1_048_576`
- `gb = 1_000_000_000`
- `gib = 1_073_741_824`

Child cache profiles override sizing fields they set and inherit fields they omit. Local cache patches change only the provided fields.

### Rate limit

Rate-limit profiles may be declared flat or inside `policies { ... }`.

```rust
rate_limit tenant {
    bucket method by [host, endpoint, method, "tenant", tenant_key] {
        cost 2
        10 / 1s
    }
}
```

Supported rate-limit keys:

- `host`
- `endpoint`
- `method`
- named keys from `rate_limit key name = arg`
- static string key atoms such as `"tenant"`

Buckets support `cost N` and shorthand windows such as `10 / 1s` or `100 / 1m`.

Attachments:

```rust
rate_limit app
rate_limit [app, method]
rate_limit only app
rate_limit off
rate_limit {
    bucket endpoint by [endpoint] {
        5 / 1s
    }
}
```

`rate_limit [a, b]` lists must contain at least one profile and cannot contain a duplicate name within the same list. Reusing a rate-limit profile across separate defaults, scopes, endpoints, or behaviors remains valid.

`rate_limit {}` is rejected because an empty inline rate-limit block has no effect. Use `rate_limit off` to clear inherited policy, or include at least one bucket.

Contextual key bindings are declared where the variables are visible:

```rust
rate_limit key tenant_key = tenant_id
```

Observers are client policy declarations:

```rust
observe rate_limit ProviderRateLimitHeaders
```

Rate-limit profiles may use `extends parent`.

### Behaviors

Behavior declarations may be flat or grouped:

```rust
behaviors {
    behavior protected_read extends read {
        auth bearer session
        cache standard
        retry read
        rate_limit [app, method]
    }
}
```

Behavior bodies support only `auth`, `cache`, `retry`, and `rate_limit`. Behavior use syntax:

```rust
behavior protected_read
behavior [read, protected_read]
```

Behavior lists must contain at least one behavior and cannot contain a duplicate name within the same list. The same behavior also cannot be attached more than once at the same defaults, scope, or endpoint site, even across separate `behavior` clauses. Reusing a behavior across separate layers remains valid.

### Request policy clauses

`query` and `headers` blocks can appear in clients, scopes, and endpoints.

```rust
query {
    count
    "startTime" = start_time
    "tag" += primary_tag
    -"debug"
}

headers {
    "User-Agent" = "ExampleApi/1.0"
    "X-Trace" = fmt["trace-", trace_id]
    -"X-Debug"
}
```

Inline forms are also supported:

```rust
query "tenant" = tenant_id
query "tag" += tag
query "debug" -
header "X-Request-Id" = request_id
header "X-Debug" -
```

Query shorthand uses the Rust argument name as both key and value. Optional query values remove the key when absent. `+=` is query-only; headers support set and remove, not append.

Query and header clauses preserve source order at a given layer. Removing a query key removes every matching entry at that layer, and a later `query` assignment for the same key appends a new entry at the end of the logical query list.

At the same declaration layer, distinct auth header/query declarations from inline forms and `auth { ... }` blocks merge in declaration order. Duplicate auth headers at the same layer are rejected case-insensitively. Duplicate auth query parameter names at the same layer are rejected by exact key match. These checks do not change normal cross-layer inheritance.

`fmt[...]` builds one wire atom from literals and variables. Optional pieces inside `fmt[...]` require all referenced optional values to be present.

```rust
path [fmt["org-", org_id]]
query { "range" = fmt[start, "-", count] }
headers { "X-Trace" = fmt["trace-", trace_id] }
```

Public `fmt[...]` pieces follow the same secret boundary as other public
request-shaping expressions: use endpoint arguments or supported safe client
variable references, not auth secrets or arbitrary generated implementation
locals.

`timeout` can be attached at a scope or endpoint.

```rust
timeout: std::time::Duration::from_secs(5)
```

### Endpoint clauses

Endpoint leaves support:

- `as alias`
- `path [...]`
- `query { ... }` or inline `query ...`
- `headers { ... }` or inline `header ...`
- `timeout: expr`
- `rate_limit key name = arg`
- `paginate Controller { ... }`
- `behavior ...`
- `auth ...`
- `retry ...`
- `cache ...`
- `rate_limit ...`
- `-> Codec<T>`
- `map Type { expr }` after the response line

The response line closes the normal endpoint contract. `map` is the documented exception because it transforms the decoded response into credential or facade output material.

### Pagination

Built-in controllers:

- `OffsetLimitPagination`
- `CursorPagination`

Custom controllers are Rust type paths implementing the pagination traits.

```rust
paginate custom::HeaderCursorPagination
```

Built-in pagination controllers use assignment blocks for their configured fields.
Custom pagination controllers use `paginate TypePath` without a block; their
state and request mutation live in the Rust controller implementation.

Response page types can implement `PageItems`; cursor page types also implement `HasNextCursor`.
Implementing `PageItems` alone does not make every endpoint paginated. The
endpoint must declare `paginate ...` before generated request builders expose
`.paginate(PaginationTermination::...)`.

Paginated endpoints cannot have request bodies in v1. The macro rejects
`body: Codec<T>` together with a `paginate` declaration because Concord does
not replay endpoint request bodies across page requests.

## Compatibility syntax

The grouped form is preferred for larger clients, but the flat form remains valid:

- `secret` and `credential` may be written directly in `client`
- `retry`, `cache`, and `rate_limit` profile declarations may be written directly in `client`
- `behavior` profiles may be written directly in `client`
- `default { ... }` remains valid beside the preferred `defaults { ... }`

Use grouped config when the client has enough policy/auth/behavior declarations that structure improves readability.

## Unsupported or reserved syntax

- There is no `body ...` endpoint clause. Request bodies are endpoint signature arguments named `body`.
- `params { ... }` blocks are not supported; scope parameters are declared in `scope name(...)`.
- `prefix` is not public v1 syntax.
- `part[...]` is not public syntax; use `fmt[...]`.
- `auth none`, `auth any`, and `auth all` are reserved and rejected.
- `behaviors { ... }` accepts only `behavior` declarations.
- `auth { ... }` accepts only `secret` and `credential` declarations.
- `policies { ... }` accepts policy/profile declarations and `observe`; default attachments belong in `defaults { ... }` or `default { ... }`.
- `profile` and `access_token` are reserved/internal keywords, not standalone public DSL clauses.

## Design rules

- Keep endpoint leaves readable.
- Keep the response line last in normal endpoints.
- Prefer grouped client config for large clients: `auth { ... }`, `policies { ... }`, `behaviors { ... }`, and `defaults { ... }`.
- Keep `default { ... }` valid, but prefer `defaults { ... }` when the block is meant to read as grouped configuration.
- Attach behaviors and policies at the narrowest scope that needs them.
- Use behaviors for semantic request patterns, not for mechanical details that are already covered by a reusable policy profile.
- See `docs/design_invariants.md` for the more detailed design invariants that should stay true as the DSL evolves.
