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
- `scope` groups route fragments, host fragments, auth, profiles, and policy attachments.
- An endpoint leaf defines one HTTP operation and its typed response.

## Client Configuration

Client-level configuration is where reusable declarations live. Attachments happen later, at default, scope, or endpoint sites.

### Declarations And Attachments

Use declarations to define reusable profiles:

- `secret token: String`
- `credential session = bearer(secret.token)`
- `retry read { ... }`
- `rate_limit app { ... }`
- `profile read { ... }`

Use attachments to apply those profiles:

- `auth bearer session`
- `retry read`
- `rate_limit app`
- `profile read`

Declarations belong in client-level config, usually grouped under `auth { ... }`, `policies { ... }`, or `profiles { ... }`. Attachments belong in `default { ... }`, scopes, or endpoints. `policies { ... }` is for declarations and observers, not default attachments.

### Canonical Client Example

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

        profiles {
            profile read {
                auth bearer api_token
                retry read
                rate_limit app
            }
        }

        default {
            profile read
        }
    }

    GET Me
    path ["me"]
    -> Json<User>
}
```

A compile-checked version of the guide examples lives in `concord_examples/src/docs_dsl.rs`. Less common public syntax is compile-checked in `concord_examples/src/docs_advanced_dsl.rs`.

### Base URL

`base` declares the scheme and root domain. It accepts only `http://...` or `https://...` host-only literals. Path, query, fragment, userinfo, backslash, whitespace, and control characters are rejected.

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

This is equivalent to writing `secret` and `credential` directly in the client block. Auth use clauses such as `auth bearer session` do not belong in `auth { ... }`; they belong in default, scopes, endpoints, or profiles.

`auth certificate` is an attachment form for client-certificate credential material. The DSL does not provide a certificate constructor in v1. Use endpoint-backed or runtime-provided credential material when certificate auth is needed.

### Policies

Retry and rate-limit profile declarations, plus policy observers, can be grouped under `policies { ... }`.

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

### Profiles

Profiles are semantic bundles over auth, retry, and rate-limit policy.

```rust
client ExampleApi {
    base "https://api.example.com"

    profiles {
        profile read {
            auth bearer session
            retry read
            rate_limit app
        }
    }
}
```

Profiles are a name layer over ordinary policy attachments. They do not change runtime semantics; they make repeated request patterns easier to read and move.

Merge rules:

- client defaults apply first
- scope policies apply from outer to inner
- endpoint policies apply last
- profile clauses at one site apply in source order
- the same profile name may be reused across different layers, but not more than once at the same default, scope, or endpoint site
- explicit `retry` overrides profile-provided `retry` at the same attachment site
- `retry off` clears inherited policy
- profile-provided `rate_limit` combines with explicit local `rate_limit`
- `rate_limit off` clears inherited rate-limit policy
- profile rate-limit key bindings are resolved where the profile is attached
- auth uses append in source order across client default, scopes, and endpoints
- profile names are preserved only as rustdoc labels; they do not affect resolved runtime policy

Profiles can also be grouped:

```rust
profiles {
    profile read {
        retry read
        rate_limit app
    }
}
```

This is the canonical grouped form for profile declarations.

A profile can extend another profile. Parent auth uses are inherited, parent rate-limit profiles are combined, and child `retry` overrides parent `retry`.

```rust
profile read {
    retry read
    rate_limit app
}

profile protected_read extends read {
    auth bearer session
}
```

Profile `rate_limit` clauses are resolved where the profile is attached. This lets a profile carry the rate-limit profile while the endpoint supplies a contextual key binding.

```rust
rate_limit match_bucket {
    bucket method by [match_key] {
        5 / 1s
    }
}

profile match_read {
    rate_limit match_bucket
}

GET Match(match_id: String)
path ["matches", match_id]
rate_limit key match_key = match_id
profile match_read
-> Json<MatchDto>
```

Attaching `match_read` as a client default would fail because `match_id` is an endpoint variable and is not available at the client level.

### Defaults

Default attachments use a singular `default { ... }` block.

```rust
default {
    profile read
    retry read
    rate_limit app
    auth bearer session
}
```

`default` applies client-wide defaults before scope and endpoint attachments. A default profile applies before explicit default `retry` and `rate_limit` clauses.

Use `rate_limit off` or `retry off` on a narrower layer to clear inherited policy.

Only one `default` block is allowed per client.

## Scopes

Scopes shape the tree below the client root. They add route fragments, host fragments, and inherited attachments for their children.

```rust
scope users {
    path ["users"]
    profile scope_read

    GET Me
    path ["me"]
    -> Json<User>
}
```

Scope-level attachments apply to all nested endpoints. Nested scopes inherit outer scopes, so scope policies flow from outer to inner before endpoint attachments are applied.

### Host And Path

`host` is scope-level v1 syntax. `host` and `path` atoms may be string literals, variables, or `fmt[...]` atoms.

```rust
scope tenant(tenant_id: String) {
    host [fmt["tenant-", tenant_id], "api"]
    path ["tenants", tenant_id]
}
```

Dynamic host pieces are label-safe only: they are validated as host labels and cannot inject scheme, userinfo, port, query, fragment, slash, backslash, whitespace, or empty/dashed labels.

Path atoms follow two rules:

- string atoms are trusted route literals and are joined raw, so a literal like `"a/b"` intentionally contributes a slash-separated route fragment;
- dynamic atoms, including `fmt[...]` in a path position, are treated as one segment of data, reject `/`, `\`, `.` and `..`, and percent-encode other bytes segment-by-segment.

### Formatting With `fmt`

`fmt[...]` builds one wire atom from literals and variables.

```rust
path [fmt["org-", org_id]]
headers { "X-Trace" = fmt["trace-", vars.trace_id] }
query { "range" = fmt[start, "-", count] }
```

Ordinary route, query, header, timeout, and pagination expressions are public request-shaping expressions. They may use endpoint arguments, declared client variables in supported reference positions, literals, and pure Rust expressions that do not reference Concord generated internals. They cannot access credential secrets, auth material, generated locals such as `cx`, `ep`, `vars`, `self`, or `request`, raw-identifier variants for restricted namespaces, or secret exposure methods.

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
profile ...
retry/rate_limit/auth ...
-> Json<Response>
```

Request bodies are endpoint signature arguments named `body`, for example `POST Create(body: Json<CreateUser>)`.

The response line should normally be the final line of the endpoint contract. Policy and profile attachments come before the response line so the endpoint leaf stays visually closed by its return type.

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

Optional arguments may also have default.

```rust
GET Search(region?: String = "euw1".to_string())
path ["search"]
query { region }
-> Json<SearchResult>
```

They initialize as `Some(default)` and can still be cleared.

### Bodies

Bodies are endpoint signature arguments. The argument name must be `body`, and the codec wraps the Rust body type.

```rust
POST Create(body: Json<CreateItem>)
path ["items"]
-> Json<Item>
```

### Response Output
Endpoint output is exactly the decoded response entity output. Profile names also appear on endpoint docs. Generated endpoint documentation includes attached profile names from client default, scopes, and endpoints.

### Auth Keyword Reference

`auth certificate` is an attachment form for client-certificate credential material. The DSL does not provide a certificate constructor in v1. Use endpoint-backed or runtime-provided credential material when certificate auth is needed.

Protected-request refresh profile treats `401 Unauthorized` and `403 Forbidden` as refreshable rejection statuses when the credential can be reacquired. Refresh tries are bounded by `max_auth_retries`.

Secret-bearing auth values are redacted from debug and display output, diagnostics, and generated documentation.

The actual outbound request still carries the credential material required by the remote API.

## Pagination

Pagination is declared on endpoints with a Rust controller type and controller field assignments.

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
paginate CursorPagination<String> {
    cursor = cursor,
    per_page = count
}
-> Json<CursorPage>
```

Pagination remains an endpoint concern. It is not part of grouped policy or profile declarations.

## Generated Documentation

Generated endpoint documentation is derived from the resolved semantic model, not from raw syntax. That is why profile names remain visible in rustdoc even though profile semantics are lowered into ordinary auth, retry, and rate-limit data.

## Keyword Reference

This section is the compact v1 reference for public DSL syntax. Focused guides such as `docs/auth.md`, `docs/retry_and_rate_limit.md`, `docs/pagination.md`, and `docs/customization.md` expand on these forms.

### Tree And Routing

- `api! { ... }` wraps one API tree.
- `client Name { ... }` declares the generated client root.
- `base "https://api.example.com"` declares the scheme and root domain.
- `scope name(args...) { ... }` groups route fragments and inherited attachments.
- `host [...]` appends scope-level host labels before the base domain.
- `path [...]` appends encoded path segments.
- Endpoint methods use HTTP method identifiers such as `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, and `OPTIONS`.

### Client Configuration

Preferred large-client grouping:

- `auth { ... }` contains `secret` and `credential` declarations.
- `policies { ... }` contains `retry` and `rate_limit` profile declarations plus `observe rate_limit`.
- `profiles { ... }` contains `profile` declarations.
- `default { ... }` contains client-wide attachments.

Flat declaration forms remain valid:

- `secret`, `credential`, `retry`, `rate_limit`, and `profile` declarations may be written directly in `client`.

### Variables And Arguments

- `var name: Type` declares a client variable available as `vars.name`.
- `secret name: Type` declares a redacted client input available only to credential declarations.
- Scope and endpoint arguments are written in the signature.
- Optional arguments use `?`.
- Defaulted arguments use `=`.
- Request bodies are endpoint arguments named `body`.

### Auth

Credential declarations:

```rust
auth {
    secret token: String

    credential api = api_key(secret.token)
    credential session = bearer(secret.token)
    credential acquired = endpoint auth_api::Login
}
```

Basic, OAuth2 client-credentials, endpoint-backed, and certificate credential material follow the same boundary: secret inputs are consumed inside credential declarations, while public request-shaping expressions cannot read them directly.

Auth attachments:

```rust
auth bearer session
auth header "X-Api-Key" = api
auth query "api_key" = api
auth basic login
auth certificate client_cert
```

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

### Rate Limit

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

`rate_limit [a, b]` lists must contain at least one profile and cannot contain a duplicate name within the same list. Reusing a rate-limit profile across separate default, scope, endpoint, or profile attachment sites remains valid.

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

### Profiles

Profile declarations may be flat or grouped:

```rust
profiles {
    profile protected_read extends read {
        auth bearer session
        retry read
        rate_limit [app, method]
    }
}
```

Profile bodies support only `auth`, `retry`, and `rate_limit`. Profile use syntax:

```rust
profile protected_read
profile [read, protected_read]
```

Profile lists must contain at least one profile and cannot contain a duplicate name within the same list. The same profile also cannot be attached more than once at the same default, scope, or endpoint site, even across separate `profile` clauses. Reusing a profile across separate layers remains valid.

### Request Policy Clauses

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

Query and header clauses preserve source order at a given layer. Removing a query key removes every matching value at that layer, and a later `query` assignment for the same key appends a new value at the end of the logical query list.

At the same declaration layer, distinct auth header/query declarations from inline forms and grouped blocks merge in declaration order. Duplicate auth headers at the same layer are rejected case-insensitively. Duplicate auth query parameter names at the same layer are rejected by exact key match. These checks do not change normal cross-layer inheritance.

`fmt[...]` builds one wire atom from literals and variables. Optional pieces inside `fmt[...]` require all referenced optional values to be present.

```rust
path [fmt["org-", org_id]]
query { "range" = fmt[start, "-", count] }
headers { "X-Trace" = fmt["trace-", trace_id] }
```

Public `fmt[...]` pieces follow the same secret boundary as other public request-shaping expressions: use endpoint arguments or supported client variable references, not credential secrets or arbitrary generated implementation locals.

`timeout` can be attached at a scope or endpoint.

```rust
timeout: std::time::Duration::from_secs(5)
```

### Endpoint Clauses

Endpoint leaves support:

- `as alias`
- `path [...]`
- `query { ... }` or inline `query ...`
- `headers { ... }` or inline `header ...`
- `timeout: expr`
- `rate_limit key name = arg`
- `paginate Controller { ... }`
- `profile ...`
- `auth ...`
- `retry ...`
- `rate_limit ...`
- `-> Codec<T>`
The response line closes the normal endpoint contract.

### Pagination

Built-in controllers:

- `OffsetLimitPagination`
- `PagedPagination`
- `CursorPagination<String>`

Custom pagination uses the uniform `paginate Type { ... }` syntax.

```rust
GET ListItems(page: u64 = 0, count: u64 = 100)
    as list_items
    path ["items"]
    query {
        "page" = page,
        "count" = count,
    }
    paginate custom::HeaderCursorPagination {
        page = page,
        count = count
    }
    -> Json<Page<String>>
```

Built-in pagination controllers use assignment blocks for their configured fields. Custom pagination uses the same assignment blocks, and endpoint planning renders query, header, path, and body output.

Built-in pagination controller fields are sema-validated against the actual semantic model.

Response page types can implement `PageItems`; cursor page types also implement `HasNextCursor`. Implementing `PageItems` alone does not make every endpoint paginated. The endpoint must declare `paginate ...` before generated request builders expose `.paginate(PaginationTermination::...)`.

Paginated endpoints cannot have request bodies in v1. The macro rejects `body: Codec<T>` together with a `paginate` declaration because Concord does not replay endpoint request bodies across page requests.

## Compatibility Syntax

The grouped form is preferred for larger clients, but the flat form remains valid:

- `secret` and `credential` may be written directly in `client`
- `retry` and `rate_limit` profile declarations may be written directly in `client`
- `profile` declarations may be written directly in `client`

Use grouped config when the client has enough policy, auth, or profile declarations that structure improves readability.

## Unsupported Or Reserved Syntax

- There is no `body ...` endpoint clause. Request bodies are endpoint signature arguments named `body`.
- `params { ... }` blocks are not supported; scope parameters are declared in `scope name(...)`.
- `prefix` is not public v1 syntax.
- The older bracketed interpolation form is not public syntax; use `fmt[...]`.
- The unsupported auth-combinator spellings are reserved and rejected.
- `profiles { ... }` accepts only `profile` declarations.
- `auth { ... }` accepts only `secret` and `credential` declarations.
- `policies { ... }` accepts policy declarations and `observe`; default attachments belong in `default { ... }`.
- `behavior`, `behaviors`, and `defaults` are legacy spellings and are rejected with migration diagnostics.
- `access_token` is reserved for credential declarations and is not a standalone public DSL clause.

## Design Rules

- Keep endpoint leaves readable.
- Keep the response line last in normal endpoints.
- Prefer grouped client config for large clients: `auth { ... }`, `policies { ... }`, `profiles { ... }`, and `default { ... }`.
- Attach profiles and policies at the narrowest scope that needs them.
- Use profiles for semantic request patterns, not for mechanical details that are already covered by a reusable policy profile.
- See `docs/design_invariants.md` for the more detailed design invariants that should stay true as the DSL evolves.
