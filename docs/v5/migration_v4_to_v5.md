# Migration: Concord v4 To v5

v5 is stricter. Removed syntax is accepted only far enough to produce a clear
diagnostic.

## Endpoint Block To Endpoint Stanza

Old endpoint outer block:

```rust
GET Broken {
    path ["broken"]
    -> Json<String>
}
```

v5:

```rust
GET Broken
    path ["broken"]
    -> Json<String>
```

Compiler message:

```text
endpoint declarations must use `METHOD Name(...) -> Response { ... }` (or `METHOD Name -> Response { ... }`)
```

## `part[...]` To `fmt[...]`

Old:

```rust
path [part["u-", id]]
```

v5:

```rust
path [fmt["u-", id]]
```

Compiler message:

```text
part[...] was renamed to fmt[...] in v5
```

## `attempts` To `max_attempts`

Old:

```rust
retry read {
    attempts 2
}
```

v5:

```rust
retry read {
    max_attempts 2
}
```

Compiler message:

```text
`attempts` was renamed to `max_attempts` in v5
```

## Multiple Defaults To One Default

Old:

```rust
default { cache short }
default { retry read }
```

v5:

```rust
default {
    cache short
    retry read
}
```

Compiler message:

```text
multiple default blocks are not allowed in v5
```

## Query Shorthand

Verbose:

```rust
query {
    "count" = count
}
```

v5 preferred style when the key matches the field:

```rust
query {
    count
}
```

Use explicit key mapping only when the wire key differs:

```rust
query {
    "startTime" = start_time
}
```

## `use_auth` To `auth ...`

Old:

```rust
use_auth HeaderAuth("X-Api-Key")
```

v5:

```rust
auth header "X-Api-Key" = key
auth bearer session
```

Compiler message:

```text
`use_auth` was removed in v5; use `auth header/query/bearer/basic/certificate ...`
```

## `auth { credential ... }` To Client Credential Lines

Old:

```rust
auth {
    credential key = api_key(secret.token)
}
```

v5:

```rust
credential key = api_key(secret.token)
```

Compiler message:

```text
`auth { credential ... }` was removed in v5; use `credential name = ...` in the client body
```

## `response custom` To `observe rate_limit`

Old:

```rust
response custom MyObserver
```

v5:

```rust
observe rate_limit MyObserver
```

Compiler message:

```text
`response custom` was removed in v5; use `observe rate_limit MyObserver`
```

## `route.host` To `host`

Old:

```rust
bucket application by [route.host] {
    500 / 10s
}
```

v5:

```rust
bucket application by [host] {
    500 / 10s
}
```

Compiler message:

```text
`route.host` was removed in v5 rate-limit keys; use `host`
```

## `auth any/all` Unsupported

Old:

```rust
auth any { bearer a bearer b }
```

v5 initial release does not support auth groups. Write multiple auth lines for
required auth.

Compiler message:

```text
auth any/all groups are not supported in v5; write multiple auth lines for required auth
```

## Custom Auth Placement Unsupported

Old:

```rust
auth custom<MyUsage>(MyUsage, key)
```

v5 initial release supports bearer/header/query/basic/certificate placement.

Compiler message:

```text
custom auth placement is not supported in v5; use bearer/header/query/basic/certificate placement instead
```

Custom auth credentials are also rejected:

```text
custom auth credentials are not supported in v5 yet; implement a CredentialProvider plus bearer/header/query/basic/certificate placement instead
```

## Explicit Endpoint API Is Advanced

The generated facade is the primary API:

```rust
let user = api.users().get(42).await?;
```

Explicit endpoint structs remain available for generic code:

```rust
let endpoint = users_api::endpoints::users::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```
