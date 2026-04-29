# 3. Generated Usage

Normal application code imports the prelude:

```rust
use concord_core::prelude::*;
```

The generated API is facade-first. The DSL tree becomes the call tree:

```rust
let api = users_api::UsersApi::new();

let user = api
    .users()
    .get(42)
    .await?;
```

Required values are direct arguments. Optional and defaulted values are setters:

```rust
let posts = api
    .posts()
    .list()
    .user_id(1)
    .debug_level(DebugLevel::V)
    .await?;
```

Endpoint-backed credentials generate acquire helpers:

```rust
api.auth_api()
    .login_for_session(login)
    .acquire_as_session()
    .await?;
```

Explicit endpoint structs remain available for advanced/generic code:

```rust
let endpoint = users_api::endpoints::users::GetUser::new(42);
let user = api.request(endpoint).execute().await?;
```

Generated rustdoc is part of the DX surface. Client methods, facade methods,
request setters, auth acquire helpers, and endpoint structs include docs for
method, path, auth, and runtime policy summaries where available.
