# Concord v5 DSL Contract

The DSL describes the upstream API contract. Runtime deployment details belong in generated configuration and core extension points, not in endpoint syntax.

## Endpoint Stanza

```rust
GET Me
    as me
    path ["me"]
    -> Json<User>
```

Required endpoint values are direct parameters:

```rust
GET GetUser(id: u64)
    as get
    path ["users", id]
    -> Json<User>
```

Optional and defaulted values are declared in the signature and become request setters:

```rust
GET Search(q: String, page?: u32, count: u32 = 20)
    as search
    path ["search"]
    query {
        q
        page
        count
    }
    -> Json<SearchResponse>
```

## Structured Formatting

`fmt[...]` is structured formatting. It creates one host label, path segment, query value, or header value:

```rust
host [fmt["tenant-", tenant_id], "api"]
path ["users", fmt["u-", user_id]]
query { "range" = fmt[start, "-", count] }
headers { "x-trace" = fmt["trace-", trace_id] }
```

## Query Shorthand

Use shorthand when the key and value name match:

```rust
query {
    count
    page
    "startTime" = start_time
}
```

## Retry

Retry profiles use `max_attempts`, which counts the initial request:

```rust
retry read {
    max_attempts 2
    methods [GET]
    on [429, 500, 503]
    retry_after
}
```

## Defaults

Use one `default` block per client or scope:

```rust
default {
    retry read
    rate_limit app
}
```

## Explicit Complexity

Supported complex behaviors are explicit:

- auth uses `auth bearer`, `auth header`, `auth query`, `auth basic`, or `auth certificate`;
- pagination uses concrete `offset_limit`, `cursor`, or `paged` plans;
- cache/retry/rate-limit are named policies or endpoint-local patches;
- response mapping uses `map Type { ... }` after the response clause.
