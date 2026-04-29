# 3. Client Blocks

A `client` block is the root of an API definition.

It defines:

- the base URL;
- constructor values;
- secrets;
- credential sources;
- reusable policy profiles;
- default inherited policies.

## Base URL

```rust
client Api {
    base https "example.com"
}
```

`base` has two parts:

```text
scheme domain
```

Supported schemes:

```rust
base https "example.com"
base http "localhost"
```

Scopes can add host labels:

```rust
client RiotClient {
    base https "riotgames.com"
}

scope platform(platform: PlatformRoute) {
    host [platform, "api"]
}
```

If `platform` formats as `euw1`, the host becomes:

```text
euw1.api.riotgames.com
```

## Variables

Use `var` for non-secret runtime configuration.

```rust
client Client {
    base https "typicode.com"

    var subdomain: String = "jsonplaceholder".to_string()
    var client_trace: bool
}
```

Required vars become constructor arguments.

Defaulted vars do not.

```rust
let api = client::Client::new(true);
```

The generated client can expose setters for vars depending on the generated wrapper.

## Secrets

Use `secret` for sensitive values.

```rust
client RiotClient {
    base https "riotgames.com"

    secret api_key: String
}
```

Secrets are stored internally as `SecretString`.

Use secrets through credentials:

```rust
credential riot = api_key(secret.api_key)
```

Avoid putting secrets directly into headers or query unless the API genuinely requires it and no auth placement fits.

## Credentials

Credential declarations define where credential material comes from.

```rust
credential key = api_key(secret.api_key)
credential bearer = bearer(secret.access_token)
credential admin = basic(secret.username, secret.password)
credential session = endpoint auth_api::LoginForSession
```

Credential declarations do not automatically apply auth to requests.

Auth is applied with `auth ...` on the client, scope, or endpoint:

```rust
auth header "X-Api-Key" = key
auth bearer session
```

## Default policy

Use `default { ... }` for inherited behavior.

```rust
client Api {
    base https "example.com"

    default {
        header "user-agent" = "Api/1.0"
        retry read
        rate_limit app
    }

    retry read {
        max_attempts 2
        methods [GET]
        on [503]
        retry_after
    }

    rate_limit app {
        bucket application by [host] {
            500 / 10s
        }
    }
}
```

`default` is useful when a profile should apply broadly.

## Multiple default blocks

Prefer one default block for readability:

```rust
default {
    retry read
    rate_limit app
}
```

If several default blocks are accepted by the parser, treat them as style debt and consolidate in examples.

## Runtime configuration

Generated clients expose runtime configuration methods for things that belong to the runtime rather than the DSL.

Typical runtime concerns:

- custom transport;
- debug level;
- cache store;
- rate limiter;
- pagination caps;
- runtime hooks.

Use the generated wrapper methods when available:

```rust
let api = client::Client::new(true)
    .with_debug_level(DebugLevel::V);
```

For tests, use `new_with_transport`:

```rust
let api = client::Client::new_with_transport(true, mock_transport);
```

## Constructor order

Constructor arguments are generated from required vars and required secrets.

Example:

```rust
client Client {
    base https "example.com"

    var trace: bool
    var tenant: String = "public".to_string()

    secret api_key: String
}
```

Constructor:

```rust
let api = client::Client::new(trace, api_key);
```

Defaulted `tenant` is not required.
