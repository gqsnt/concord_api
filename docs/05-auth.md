# 5. Auth

Normal auth usage imports the prelude:

```rust
use concord_core::prelude::*;
```

Declare secrets and credentials in the client body:

```rust
client Api {
    base https "example.com"

    secret api_key: String
    credential key = api_key(secret.api_key)
}
```

Use credentials through policy lines on a client, scope, or endpoint:

```rust
scope protected {
    auth header "X-Api-Key" = key

    GET Me
        as me
        path ["me"]
        -> Json<User>
}
```

Endpoint-backed sessions generate `acquire_as_*` helpers:

```rust
api.auth_api()
    .login_for_session(login)
    .acquire_as_session()
    .await?;

let me = api.protected().me().await?;
api.auth_state().session().clear().await;
```

Advanced credential providers are extension points and import from
`concord_core::advanced::*`.

Unsupported in v5 initial release:

- `auth any { ... }`
- `auth all { ... }`
- custom auth placement in the DSL
