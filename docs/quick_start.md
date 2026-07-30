# Quick start

Concord requires Rust 1.86 or newer.

## Add the dependency

Use the façade crate by path:

```toml
[dependencies]
concord = { path = "../concord_api/crates/concord", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

The `json` feature exposes the `Json<T>` request and response specification.

## Declare a client

```concord
use concord::{Json, api};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct User {
    pub id: u64,
    pub name: String,
}

api! {
    client UsersApi {
        base "https://users.example.test/";
    }

    GET user(id: u64) -> Json<User> {
        path ["v1", "users", ep.id];
    }
}
```

One `api!` invocation declares one client. Endpoint parameters without
defaults are method arguments in declaration order.

## Build and call

```rust
# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let client = UsersApi::new()?;
let user: User = client.user(42).await?;
let response = client.user(42).response().await?;
# let _ = (user, response);
# Ok(())
# }
```

`UsersApi::new()` exists because the declaration has no required client value,
secret, or provider. `UsersApi::builder()` is available for runtime
configuration.

`.await` returns the declared output. `.response().await` returns accepted
response metadata with the declared output.

## Add a client value

```concord
api! {
    client RegionalApi {
        base "https://example.test/";
        vars {
            region: String;
        }
        host [vars.region, "api"];
    }

    GET health() -> Json<serde_json::Value> {
        path ["health"];
    }
}
```

Build the required value through its generated setter:

```rust
let client = RegionalApi::builder()
    .set_region("eu".to_owned())
    .build()?;
```

## Add a secret

```concord
api! {
    client PrivateApi {
        base "https://private.example.test/";
        credentials {
            secret token: String;
            credential bearer_token = bearer(secret.token);
        }
        auth bearer bearer_token challenge never;
    }

    GET account() -> Json<serde_json::Value>;
}
```

Supply the generated secret input with `SecretString`:

```rust
let token = concord::SecretString::try_new("application-secret")?;
let client = PrivateApi::builder().token(token).build()?;
```

Secret material is placed only at the private HTTP boundary.

## Next references

- [DSL specification](spec.md) defines every declaration rule.
- [Generated client](generated_client.md) defines generated names and methods.
- [Runtime model](runtime.md) defines execution order and observation.
- [Authentication](auth.md) defines credential runtime behavior.
