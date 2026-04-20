# DSL Reference

This is the compact syntax reference for the current Concord DSL.

## Top-Level Shape

```rust
api! {
    client Api {
        scheme: https,
        host: "example.com",
    }

    scope users {
        path["users"]

        GET GetUser(id: u64) -> Json<User> {
            path[id]
        }
    }
}
```

The required architecture is `client -> scope -> endpoint`. Root endpoints are allowed, but scopes are preferred for real APIs.

## Canonical Block Order

Recommended order keeps large APIs readable:

```rust
client Api {
    scheme: https,
    host: "example.com",
    vars { ... }
    secret { ... }
    auth { ... }
    use_auth ...
    headers { ... }
    query { ... }
    timeout: ...,
    retry { ... }
    cache { ... }
    rate_limit { ... }
}

scope family(params...) {
    host[...]
    path[...]
    rate_limit key name = param
    use_auth ...
    headers { ... }
    query { ... }
    timeout: ...,
    retry ...
    cache ...
    rate_limit ...
    scope child { ... }
    GET Endpoint(...) -> Json<T> { ... }
}

GET Endpoint(params..., body: Json<Body>) -> Json<Response> | Out => { ... } {
    path[...]
    headers { ... }
    query { ... }
    timeout: ...,
    use_auth ...
    retry ...
    cache ...
    rate_limit ...
    paginate Controller { ... }
}
```

The parser accepts flexible ordering where the DSL defines it, but this is the canonical spelling for docs and examples: identity first, route second, policy third, lifecycle features last.

## Client Block

```rust
client Api {
    scheme: https,
    host: "example.com",

    vars { tenant: String = "public".to_string() }
    secret { api_key: String }

    auth { credential key: ApiKey(secret.api_key) }
    use_auth HeaderAuth("X-Api-Key", key)

    headers { "user-agent" = "my-client" }
    query { "lang" = "en" }
    timeout: std::time::Duration::from_secs(10),

    retry { ... }
    cache { ... }
    rate_limit { ... }
}
```

Client fields define global identity, shared values, secrets, auth credentials, and default policies.

## Variables And Secrets

```rust
vars {
    tenant: String = "public".to_string(),
    trace?: bool,
}

secret {
    api_key: String,
}
```

Required values become constructor args. Optional/defaulted values get generated setters.

Use `vars.name` for non-secret shared values and `secret.name` for secret values in allowed contexts.

## References

Reference owners are explicit:

| Spelling | Meaning | Allowed In |
| --- | --- | --- |
| `vars.x` | client variable | route, headers, query, timeout |
| `secret.x` | secret variable | auth credentials, headers/query values when explicitly used |
| `ep.x` | scope or endpoint parameter | route, headers, query, timeout, pagination assignments |
| `x` | endpoint-visible parameter in local route/policy context | endpoint route/policy shorthand |

Use owner-qualified spelling when ambiguity would hurt readability. `secret.*` is not allowed in route host/path pieces.

## Scopes

```rust
scope platform(platform: PlatformRoute) {
    host[platform, "api"]
    path["lol"]
    use_auth HeaderAuth("X-Riot-Token", riot_api_key)

    scope summoner {
        path["summoner", "summoners"]
    }
}
```

Scopes group route and policy inheritance. Scope params are inherited by child endpoints and appear first in generated constructors.

## Endpoints

```rust
GET GetUser(id: u64, trace?: bool = false) -> Json<User> {
    path[id]
    query { "trace" = trace }
}

POST Create(body: Json<NewUser>) -> Json<User> {
    path["users"]
}

GET Login -> Json<LoginResponse> | AccessToken => {
    AccessToken::new(r.access_token)
} {
    path["login"]
}
```

Endpoint signatures declare required, optional, and defaulted inputs. The response is declared after `->`. Optional mapping uses `| Out => { ... }` and receives decoded value as `r`.

Constructor order is:

1. inherited scope params from outer scope to inner scope
2. endpoint params in written order
3. required body, when present

Optional or defaulted endpoint params are set with generated endpoint setters.

## Routes

```rust
host[platform, "api"]
path["users", id]
path["prefix", part["user-", id]]
```

Route atoms are:

1. string literals
2. identifiers, interpreted as `ep.name`
3. scoped refs: `vars.name`, `ep.name`, `secret.name` where allowed
4. `part[...]` for composing one host label, path segment, header value, or query value

Dynamic path values are percent-encoded as one segment.

`part[...]` composes one value. In `path[...]`, that one value is still one path segment:

```rust
path["users", part["user-", id]]
headers { "x-trace" = part["trace-", id] }
query { "tag" += part["team-", team] }
```

## Policy Blocks

```rust
headers {
    "x-debug" = debug,
    -"x-old-header",
}

query {
    "tag" += tag,
    "trace" = trace,
    -"old",
}
```

Headers support set and remove. Query supports set, push with `+=`, and remove.

Policy blocks assign values. They do not declare params.

Policy inheritance is ordered:

1. client defaults
2. outer scope
3. inner scope
4. endpoint
5. runtime request patch, such as pagination or per-request timeout/cache mode

More specific policy can override, remove, or patch earlier policy depending on the feature.

## Auth

```rust
auth {
    credential api_key: ApiKey(secret.api_key)
    credential token: BearerToken(secret.access_token)
    credential basic: Basic(secret.username, secret.password)
    credential session: Endpoint(auth::LoginForSession)
    credential custom: Custom<MyProvider>(MyProvider::new())
}

use_auth BearerAuth(token)
use_auth HeaderAuth("X-Api-Key", api_key)
use_auth QueryAuth("api_key", api_key)
use_auth one_of [BearerAuth(token), HeaderAuth("X-Fallback", api_key)]
use_auth [BearerAuth(token), HeaderAuth("X-Extra", api_key)]
```

Credential declaration and wire application are separate concepts.

Endpoint-backed credentials are manual and explicit:

```rust
api.acquire_auth_session(endpoints::auth::LoginForSession::new(...)).await?;
```

## Retry

```rust
retry {
    profile read {
        attempts 2
        methods [GET, HEAD]
        on status[429, 500, 503]
        retry_after honor
        backoff none
    }
    default read
}

GET Ping -> Json<()> { retry read }
GET NoRetry -> Json<()> { retry off }
```

Retry policies are bounded and explicit. Unsafe methods should declare idempotency.

Retry semantics:

1. `retry profile_name` applies a named profile.
2. `retry { ... }` patches inherited retry config.
3. `retry off` disables retry for that scope or endpoint.

## Cache

```rust
cache {
    profile short {
        ttl 60 seconds
        capacity 1024 entries
        revalidate true
        on_error serve_stale
    }
    default short
}

GET Read -> Json<String> { cache short }
GET Fresh -> Json<String> { cache only short }
GET Raw -> Json<String> { cache off }
```

Per-request runtime modes are methods on the pending request:

```rust
.cache_default()
.cache_bypass()
.cache_refresh()
```

Cache semantics:

1. `cache profile_name` applies or patches inherited cache config.
2. `cache only profile_name` replaces inherited cache config with that profile.
3. `cache { ... }` patches inherited cache config.
4. `cache only { ... }` replaces inherited cache config with the inline config.
5. `cache off` disables cache for that scope or endpoint.

## Rate Limits

```rust
rate_limit {
    response custom MyResponsePolicy

    profile app {
        bucket application by [route.host] {
            cost 1
            limit 500 every 10 seconds
        }
    }

    default app
}

scope platform(platform: String) {
    rate_limit key region = platform
}

GET Ping -> Json<()> {
    rate_limit app
    rate_limit only app
    rate_limit off
}
```

Rate-limit keys describe request bucket identity. Response policies interpret limited responses and cooldown targets.

Rate-limit semantics:

1. `rate_limit profile_a profile_b` appends profiles to the inherited plan.
2. `rate_limit only profile_a` replaces the inherited plan.
3. inline `bucket ...` plans follow the same append or `only` replacement rule.
4. `rate_limit off` removes the generated rate-limit plan for that scope or endpoint.

## Pagination

```rust
GET List(offset: u64 = 0, limit: u64 = 100) -> Json<Vec<Item>> {
    query { "offset" = offset, "limit" = limit }
    paginate OffsetLimitPagination {
        offset = offset,
        limit = limit
    }
}
```

Use `.paginate()` on a pending request:

```rust
let items = api.request(endpoints::List::new()).paginate().collect().await?;
```

## Generated Endpoint Modules

Root endpoint:

```rust
GET Ping -> Json<()>;
api.request(endpoints::Ping::new()).execute().await?;
```

Scoped endpoint:

```rust
scope api { GET Ping -> Json<()>; }
api.request(endpoints::api::Ping::new()).execute().await?;
```

Scoped endpoints are not reexported at the root.

## Runtime Order

The runtime order is summarized in [Runtime Client](12-runtime-client.md) and introduced in [Introduction](01-introduction.md):

1. build route, policy, body, retry, cache, and rate-limit plan
2. prepare auth
3. return fresh cache hits before inflight, rate-limit, retry, and transport
4. send stale revalidation through the normal inflight/rate-limit/transport path
5. let auth inspect responses before cache update and decode
6. coordinate retry and rate-limit delays so the client does not double sleep

## Canonical Style

1. Put API architecture in scopes.
2. Keep endpoint signatures contract-first.
3. Put route pieces before operational policy inside endpoint blocks.
4. Use profiles for repeated retry/cache/rate-limit policy.
5. Use runtime trait extensions for environment-specific behavior instead of expanding the DSL.

## Do This, Not That

Use scope modules for real API families:

```rust
scope platform(platform: PlatformRoute) {
    host[platform, "api"]
    path["lol", "summoner"]

    GET GetSummoner(name: String) -> Json<Summoner> {
        path["summoners", "by-name", name]
    }
}
```

Call the scoped module path:

```rust
api.request(endpoints::platform::GetSummoner::new(platform, name)).execute().await?;
```

Do not flatten scoped endpoints into root names or expect root aliases for scoped endpoints.

Use declared params in policy blocks:

```rust
GET Create(idempotency_key: String) -> Json<()> {
    headers { "idempotency-key" = idempotency_key }
}
```

Do not declare new params inside `headers`, `query`, `host`, or `path`.
