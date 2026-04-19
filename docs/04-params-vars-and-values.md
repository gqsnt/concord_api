# 4. Parameters, Variables, and Values

Concord has three common value sources:

- Client variables from `vars { ... }`
- Secrets from `secret { ... }`
- Endpoint parameters from endpoint signatures
- Scope parameters from `scope name(...)`

The DSL lets you reference these values in host labels, path segments, headers, query strings, pagination controllers, auth providers, retry idempotency, and rate-limit keys.

## Declaring variables

Client-level variables are non-secret runtime configuration.

```rust
client Client {
    vars {
        subdomain: String = "jsonplaceholder".to_string(),
        client_trace: bool
    }
}
```

A declaration can be required, optional, or defaulted.

```rust
GET ListPosts(
    id: u32,
    page?: u32,
    per_page: u32 = 20
) -> Json<Vec<Post>> {
    query {
        "page" = page,
        "per_page" = per_page
    }
}
```

Required values become constructor arguments. Optional and defaulted values become builder setters on the endpoint.

```rust
GET ListPosts(user_id?: u32, x_debug: bool = true) -> Json<Vec<Post>> {
    query {
        "userId" = user_id,
        "debug" = x_debug
    }
}
```

```rust
api.request(endpoints::ListPosts::new().user_id(1).x_debug(false))
    .execute()
    .await?;
```

## Scope parameters

Scope parameters are inherited by child endpoints. Use them for region, platform, tenant, version, or any route value shared by multiple endpoints.

```rust
scope platform(platform: PlatformRoute) {
    host[platform, "api"]

    GET GetPlatformData -> Json<PlatformDataDto> {
        path["lol", "status", "v4", "platform-data"]
    }
}
```

The generated endpoint constructor includes `platform` because it is required by the parent scope.

## Secrets

Secrets are declared with `secret { ... }`.

```rust
client RiotClient {
    secret {
        api_key: String
    }
}
```

Secrets can be used directly in headers, but prefer the auth DSL for real authentication.

```rust
headers {
    "authorization" = secret.api_key
}
```

The generated client stores secrets as `SecretString` internally and provides setters such as `set_api_key(...)`.

## Value references

The DSL supports explicit references:

```rust
headers {
    "x-tenant" = vars.tenant,
    "authorization" = secret.api_key,
    "x-user" = ep.user_id
}
```

Canonical prefixes are `vars.`, `secret.`, and `ep.`.

For endpoint and scope parameters, a bare lowercase identifier is normalized to `ep.<name>` in policy expressions.

```rust
query {
    "userId" = user_id
}
```

This reads from the endpoint value named `user_id`.

## Values in routes

Route values can be static strings, references, or `part[...]` templates.

```rust
path["users", user_id]
host[vars.subdomain]
path["x", part["p", value]]
```

Use route references instead of formatting URL strings manually. Concord validates hosts and percent-encodes path segments.

## Values in policy blocks

Policy values are Rust expressions after Concord normalizes simple references.

```rust
headers {
    "x-enabled" = vars.client_trace,
    "x-debug" = part["test:", x_debug]
}

query {
    "page" = page,
    "dup" += "a",
    "dup" += "b"
}
```

Values are converted to strings for headers and query strings. Invalid header names or values become request-building errors before transport is called.

## Short binds in policy blocks

Headers and query blocks can bind request parameters directly.

```rust
POST Create(body: Json<CreateRequest>) -> Json<CreateResponse> {
    headers {
        "Idempotency-Key" as idempotency_key: String
    }
    retry write
}
```

This declares an endpoint parameter named `idempotency_key` and wires it to the header. The generated constructor receives the required value.

Short bind syntax uses an identifier key and a type.

```rust
query {
    page?: u32,
    per_page: u32 = 20
}
```

This declares `page` and `per_page` as endpoint parameters and writes them to query keys named after the identifiers. When you need exact wire spelling, use a string key with `as`.

```rust
query {
    "per_page" as per_page: u32 = 20
}
```

## Optional values

Optional values are omitted when missing.

```rust
GET Search(q?: String) -> Json<Vec<Item>> {
    query { "q" = q }
}
```

`Search::new()` sends no `q` query parameter. `Search::new().q("rust".to_string())` sends `?q=rust`.

The same rule applies to optional path segments and `part[...]` values.

## Defaulted values

A defaulted parameter is present unless the endpoint builder overrides it.

```rust
GET GetPosts(x_debug: bool = true) -> Json<Vec<Post>> {
    headers { "x-debug" = part["test:", x_debug] }
}
```

`GetPosts::new()` sends `x-debug: test:true`. `GetPosts::new().x_debug(false)` sends `x-debug: test:false`.

## Types and formatting

Values used in route, header, and query positions must be convertible to strings in generated code. In practice, simple primitive values, strings, enums with `Display`, and newtypes with `Display` work well.

For host labels, the formatted value must be a valid host label. For path segments, Concord percent-encodes after formatting.


