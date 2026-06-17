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

A compile-checked version of the guide examples lives in `concord_examples/src/docs_dsl.rs`.

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

Path atoms are encoded segment-by-segment. Split fixed path pieces into separate string atoms.

### Formatting with `fmt`

`fmt[...]` builds one wire atom from literals and variables.

```rust
path [fmt["org-", org_id]]
headers { "X-Trace" = fmt["trace-", vars.trace_id] }
query { "range" = fmt[start, "-", count] }
```

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

## Compatibility syntax

The grouped form is preferred for larger clients, but the flat form remains valid:

- `secret` and `credential` may be written directly in `client`
- `retry`, `cache`, and `rate_limit` profile declarations may be written directly in `client`
- `behavior` profiles may be written directly in `client`
- `default { ... }` remains valid beside the preferred `defaults { ... }`

Use grouped config when the client has enough policy/auth/behavior declarations that structure improves readability.

## Design rules

- Keep endpoint leaves readable.
- Keep the response line last in normal endpoints.
- Prefer grouped client config for large clients: `auth { ... }`, `policies { ... }`, `behaviors { ... }`, and `defaults { ... }`.
- Keep `default { ... }` valid, but prefer `defaults { ... }` when the block is meant to read as grouped configuration.
- Attach behaviors and policies at the narrowest scope that needs them.
- Use behaviors for semantic request patterns, not for mechanical details that are already covered by a reusable policy profile.
- See `docs/design_invariants.md` for the more detailed design invariants that should stay true as the DSL evolves.
