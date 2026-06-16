# DSL

Concord describes an HTTP API as a typed tree. The tree has one `client` root, optional `scope` branches, and endpoint stanza leaves. The macro turns that tree into a facade-first Rust client and endpoint request plans.

## API Tree

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

- `client` defines the root type, base URL, root variables, credentials, defaults, and named policy profiles.
- `scope` groups route, host, auth, and policy fragments. A scope can take parameters.
- An endpoint stanza defines one HTTP operation and its typed response.

## Base URL

A client declares its base scheme and domain with `base`.

```rust
client ExampleApi {
    base "https://api.example.com"
}
```

Use `http` or `https` for the scheme. Dynamic host labels are added with `host [...]` in scopes.

## Host And Path

`host [...]` appends host labels before the base domain. `path [...]` appends path atoms.

```rust
scope tenant(tenant_id: String) {
    host [fmt["tenant-", tenant_id], "api"]
    path ["tenants", tenant_id]
}
```

Path atoms are encoded segment-by-segment. Split fixed path pieces into separate string atoms.

## Endpoint Stanza

An endpoint stanza starts with an HTTP method and Rust endpoint name, followed by arguments in parentheses.

```rust
POST CreateUser(account_id: u64, body: Json<CreateUser>)
    as create
    path ["accounts", account_id, "users"]
    -> Json<User>
```

`as` sets the generated facade method name. Without `as`, the endpoint name is converted to snake_case.

## Endpoint Clause Order

Recommended endpoint order:

```rust
GET Name(params)
as facade_name
path [...]
query { ... }
headers { ... }
paginate Controller {
    field = value
}
cache/retry/rate_limit/auth clauses
-> Json<Response>
```

The response line should normally be the final line of the endpoint contract. This keeps endpoint leaves visually closed by their return type.

Response mapping is the exception when used:

```rust
GET Login
path ["login"]
-> Json<LoginResponse>
map AccessToken { AccessToken::new(r.access_token) }
```

## Endpoint Arguments

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

Defaulted arguments use `=` and are initialized before fluent setters run.

```rust
GET List(start: u64 = 0, count: u64 = 20)
    path ["items"]
    query { start, count }
    -> Json<Vec<Item>>
```

Bodies are endpoint signature arguments. The argument name must be `body`, and the codec wraps the Rust body type.

```rust
POST Create(body: Json<CreateItem>)
    path ["items"]
    -> Json<Item>
```

## Formatting With fmt

`fmt[...]` builds one wire atom from string literals and variables.

```rust
path [fmt["org-", org_id]]
headers { "X-Trace" = fmt["trace-", vars.trace_id] }
query { "range" = fmt[start, "-", count] }
```

Use `fmt[...]` when one host label, path segment, query value, or header value needs multiple pieces.

## Query

Query policies live in `query { ... }` blocks.

Shorthand uses the Rust field as both key and value:

```rust
query {
    count
}
```

Explicit keys use string literals:

```rust
query {
    "startTime" = start_time,
    "endTime" = end_time
}
```

Append repeated query values with `+=`:

```rust
query {
    "tag" += primary_tag,
    "tag" += secondary_tag
}
```

Remove an inherited query key with `-`:

```rust
query {
    -"debug"
}
```

Optional argument values remove their query key when absent.

## Headers

Header keys are explicit string literals.

```rust
headers {
    "User-Agent" = "ExampleApi/1.0",
    "X-Trace" = fmt["trace-", vars.trace_id]
}
```

Setting the same header in a narrower layer overrides the inherited value. Remove an inherited header with `-`.

```rust
headers {
    -"X-Trace"
}
```

## Auth

Declare secrets and credentials in the client block.

```rust
client ExampleApi {
    base "https://api.example.com"
    secret api_key: String
    secret token: String

    credential key = api_key(secret.api_key)
    credential session = bearer(secret.token)
}
```

Attach credentials as auth requirements at the client, scope, or endpoint layer.

```rust
auth header "X-Api-Key" = key
auth query "api_key" = key
auth bearer session
```

Endpoint-backed credentials store the output of one endpoint as a credential for later requests.

```rust
client SessionApi {
    base "https://example.com"
    secret upstream_key: String
    credential upstream = api_key(secret.upstream_key)
    credential session = endpoint auth_api::LoginForSession
}

scope auth_api {
    POST LoginForSession(body: Json<LoginRequest>)
        path ["login"]
        auth header "X-Upstream-Key" = upstream
        -> Json<LoginResponse>
        map AccessToken { AccessToken::new(r.access_token) }
}

scope protected {
    auth bearer session

    GET Me
        path ["me"]
        -> Json<User>
}
```

### Grouped Auth Configuration

For larger clients, secrets and credentials can be grouped:

```rust
auth {
    secret token: String
    credential session = bearer(secret.token)
}
```

This is equivalent to writing `secret` and `credential` directly in the client block.

Auth use clauses such as `auth bearer session` still belong in defaults, scopes, endpoints, or behavior profiles; they are not part of the grouped `auth { ... }` section.

## Cache, Retry, And Rate Limit Profiles

Named profiles live in the client block. Attach them with defaults, scope policies, or endpoint policies.

```rust
client PolicyApi {
    base "https://example.com"

    default {
        retry read
        cache standard
        rate_limit app
    }

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
}
```

Use `cache off`, `retry off`, or `rate_limit off` on a narrower layer to clear an inherited policy. Use `rate_limit [a, b]` to add multiple rate-limit profiles to the same endpoint.

### Grouped Policy Configuration

For larger clients, retry, cache, rate-limit profile declarations and observers can be grouped:

```rust
policies {
    retry read {
        max_attempts 2
        methods [GET]
    }

    rate_limit app {
        bucket application by [host] {
            10 / 1s
        }
    }

    observe rate_limit MyObserver
}
```

This is equivalent to writing those declarations directly in the client block.

Default policy attachments such as `retry read`, `cache standard`, or `rate_limit app` still belong in `default { ... }`, not in `policies { ... }`.

## Behavior Profiles

Behavior profiles give a semantic name to cross-cutting request behavior.

```rust
behavior protected_read {
    auth bearer session
    retry read
    cache standard
    rate_limit app
}
```

Attach behavior profiles to scopes or endpoints:

```rust
scope users {
    path ["users"]
    behavior protected_read

    GET Me
    path ["me"]
    -> Json<User>
}
```

A behavior can extend another behavior:

```rust
behavior protected_read extends read {
    auth bearer session
}
```

Behavior profiles are resolved by the macro into existing auth, cache, retry, and rate-limit policy data. The core runtime does not know about behavior syntax.

In this version, behavior profiles can contain `auth`, `retry`, `cache`, and `rate_limit` clauses.

For larger clients, behavior declarations may be grouped:

```rust
behaviors {
    behavior protected_read {
        auth bearer session
        retry read
    }
}
```

This is equivalent to writing the behavior profiles directly in the client block.

Behavior `rate_limit` clauses are resolved where the behavior is attached. This means they can use rate-limit key bindings visible at that scope or endpoint.

When a behavior supplies a rate-limit profile and a narrower scope or endpoint adds another plain `rate_limit` profile, the profiles are combined. Use `rate_limit off` to clear inherited rate-limit behavior.

Behavior profiles can also be used from the client default block:

```rust
default {
    behavior protected_read
}
```

A default behavior applies before explicit default `cache`, `retry`, and `rate_limit` clauses. Attach behavior at a scope or endpoint instead when it needs rate-limit key bindings declared in that scope or endpoint.

For larger clients, `defaults { ... }` is accepted as an alias for the client `default { ... }` block.
Only one default/defaults block is allowed per client.

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

Cursor pagination uses a response type that exposes items and a next cursor.

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

## Defaults

`default { ... }` applies named policies to every endpoint in that layer unless a narrower layer overrides or clears them.

```rust
scope protected {
    auth bearer session
    default { retry read }
}
```

Defaults are inherited through the API tree in client, scope, endpoint order.
