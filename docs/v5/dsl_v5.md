# Concord v5 DSL Contract

The DSL describes the upstream API contract. It does not describe runtime deployment details.

Canonical endpoint stanza:

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

Optional and defaulted values are declared in the endpoint signature and become request setters:

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

`fmt[...]` is structured formatting. It creates one host label, path segment, query value, or header value:

```rust
host [fmt["tenant-", tenant_id], "api"]
path ["users", fmt["u-", user_id]]
query { "range" = fmt[start, "-", count] }
headers { "x-trace" = fmt["trace-", trace_id] }
```

Query shorthand is canonical when the key and value name match:

```rust
query {
    count
    page
    "startTime" = start_time
}
```

Retry profiles use `max_attempts`:

```rust
retry read {
    max_attempts 2
    methods [GET]
    on [429, 500, 503]
    retry_after
}
```

Use one `default` block per client or node:

```rust
default {
    retry read
    rate_limit app
}
```

Non-goals for v5 initial release:

- string path syntax such as `path "/users/{id}"`
- Rust format-string parsing
- `part[...]`; use `fmt[...]`
- `auth any` / `auth all`
- custom auth placement
- generic middleware DSL
- cache backend/storage DSL
- automatic login or automatic pagination
- rate-limit syntax redesign
- generic `profiles { ... }` wrapper

Old syntax is accepted only when needed to emit a migration diagnostic.
