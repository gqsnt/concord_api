# 6. Policies: Headers, Query, and Timeout

Headers, query parameters, and timeout are policies.

Policies can be written on:

- the client;
- a scope;
- an endpoint;
- a pending request at runtime.

More specific policy wins.

## Headers

Client-level header:

```rust
client Api {
    base https "example.com"

    headers {
        "user-agent" = "Api/1.0"
        "x-client-trace" = vars.client_trace
    }
}
```

Scope-level header:

```rust
scope protected {
    header "x-area" = "protected"
}
```

Endpoint-level header:

```rust
GET GetPosts(x_debug: bool = true)
    headers {
        "x-debug" = fmt["test:", x_debug]
    }
    -> Json<Vec<Post>>
```

## Header removal

```rust
headers {
    -"x-client-trace"
}
```

Use removal when an endpoint must opt out of an inherited header.

## Query

```rust
GET GetPosts(user_id?: u32)
    query {
        "userId" = user_id
    }
    -> Json<Vec<Post>>
```

Optional values are omitted when `None`.

Defaulted values are present unless changed by the builder.

## Query append

Query supports append with `+=`.

```rust
query {
    "tag" += "a"
    "tag" += "b"
}
```

Headers do not support `+=`.

## `fmt[...]`

`fmt[...]` builds one value from several pieces.

```rust
headers {
    "x-debug" = fmt["test:", x_debug]
}

query {
    "filter" = fmt["user:", user_id]
}
```

If a `fmt[...]` references an optional value that is missing, the containing value is omitted.

## Timeout

```rust
client Api {
    base https "example.com"

    timeout std::time::Duration::from_secs(30)
}
```

Scope override:

```rust
scope fast {
    timeout std::time::Duration::from_millis(500)
}
```

Endpoint override:

```rust
GET Slow
    timeout std::time::Duration::from_secs(5)
    -> Json<Value>
```

Runtime override:

```rust
api.service()
   .slow()
   .timeout(std::time::Duration::from_secs(2))
   .await?;
```

## Policy inheritance

Example:

```rust
client Api {
    base https "example.com"

    headers {
        "user-agent" = "Api/1.0"
    }
}

scope v1 {
    path ["v1"]

    headers {
        "x-api-version" = "1"
    }

    GET Me
        as me
        path ["me"]
        -> Json<User>
}
```

`Me` receives both headers.

## Policy design guidance

Use:

- client policy for stable global defaults;
- scope policy for API family behavior;
- endpoint policy for one request;
- runtime request modifiers for one call.
