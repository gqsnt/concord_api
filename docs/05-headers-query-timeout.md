# 5. Headers, Query, and Timeout

Headers, query parameters, and timeout are policy blocks. They can be declared at the client, scope, or endpoint level.

Client policy is inherited by scopes. Scope policy is inherited by child scopes and endpoints. Endpoint policy is most specific.

## Headers

Use `headers { ... }` to set, override, bind, or remove headers.

```rust
client ApiHeaders {
    scheme: https,
    host: "example.com",

    vars {
        user_agent: String = "ua".to_string(),
        flag: bool = true
    }

    headers {
        user_agent = vars.user_agent,
        x_debug = "caribou",
        "x-static" = "s",
        "x-flag" = vars.flag
    }
}
```

Identifier keys are converted to HTTP-style names where appropriate. For example, `user_agent` becomes `user-agent`.

String keys keep exact spelling.

```rust
headers {
    "x-static" = "s"
}
```

## Header overrides and removals

More specific blocks override earlier values.

```rust
scope p_scope {
    path["p"]
    headers {
        "x-debug" = "override",
        -"x-static"
    }

    GET One -> Json<()> {
        headers { -"x-flag" }
    }
}
```

The endpoint request contains `x-debug: override`, does not contain `x-static`, and does not contain `x-flag`.

## Header binding

A header can use an endpoint parameter declared in the endpoint signature.

```rust
POST Create(idempotency_key: String) -> Json<CreateResponse> {
    headers {
        "Idempotency-Key" = idempotency_key
    }
    retry write
}
```

The generated constructor receives `idempotency_key`.

```rust
api.request(endpoints::Create::new("create-1".to_string()))
    .execute()
    .await?;
```

## Query parameters

Use `query { ... }` to set, append, bind, or remove query parameters.

```rust
client ApiQuery {
    scheme: https,
    host: "example.com",

    query {
        "sdk" = "concord",
        "dup" += "c0"
    }
}

scope x_scope {
    path["x"]
    query {
        -"sdk",
        "dup" += "s1"
    }

    GET One -> Json<()> {
        query {
            "dup" = "e1"
        }
    }
}
```

`=` replaces the current values for a key. `+=` appends another value for the same query key. `+=` is query-only and is rejected in `headers {}`.

## Query `part[...]`

`part[...]` is useful for derived query values.

```rust
GET One(v: String) -> Json<()> {
    path["x"]
    query { "q" = part["a:", v] }
}
```

This sends query key `q` with value `a:z` for `v = "z"`.

If the referenced value is optional and missing, the query key is absent.

```rust
GET One(v?: String) -> Json<()> {
    path["x"]
    query { "q" = part["a:", v] }
}
```

`One::new()` sends no `q`. `One::new().v("z".to_string())` sends `q=a:z`.

## Query binding

Bind query values from endpoint parameters declared in the endpoint signature.

```rust
GET Search(query: String, page: u32 = 1) -> Json<SearchResponse> {
    path["search"]
    query {
        "q" = query,
        "page" = page
    }
}
```

Use string keys when the wire key must contain underscores, dashes, capitalization, or other exact spelling.

## Automatic Accept and Content-Type

`Json<T>` response endpoints set `Accept: application/json` unless the endpoint or an inherited policy explicitly overrides or removes `accept`.

```rust
GET A -> Json<()>;

GET B -> Json<()> {
    headers { "accept" = "text/plain" }
}

GET C -> Json<()> {
    headers { -"accept" }
}
```

For JSON request bodies, Concord sets `Content-Type: application/json` unless policy overrides it.

```rust
POST A(body: Json<NewObj>) -> Json<()>;

POST B(body: Json<NewObj>) -> Json<()> {
    headers { "content-type" = "text/plain" }
}
```

GET endpoints without bodies do not receive a content type.

## Timeout

Use `timeout: <expr>` at the client, scope, or endpoint level.

```rust
client ApiTimeout {
    scheme: https,
    host: "example.com",
    timeout: core::time::Duration::from_secs(30)
}

scope x_scope {
    path["x"]
    timeout: core::time::Duration::from_millis(500)

    GET A -> Json<()> {
        timeout: core::time::Duration::from_secs(1)
    }
}
```

The most specific timeout wins.

At runtime, `PendingRequest` can override timeout for one request.

```rust
api.request(endpoints::A::new())
    .timeout(core::time::Duration::from_secs(2))
    .execute()
    .await?;
```

Runtime helpers:

- `timeout(duration)` sets a per-request timeout.
- `clear_timeout()` removes the effective timeout for that request.
- `inherit_timeout()` returns to the DSL-inherited timeout behavior.

## Policy design rules

Put stable defaults at the client level. Put route-family values in scopes. Put request-specific values on endpoints.

Use removal when an endpoint must opt out of an inherited value.

Use string keys when the upstream API has exact names. Use identifier keys when ordinary kebab-case header names are acceptable.

