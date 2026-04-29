# Concord v5 Generated Usage Contract

Generated usage is facade-first.

Client construction:

```rust
let api = Api::new(api_key);

let api = Api::builder()
    .api_key(api_key)
    .tenant(tenant)
    .build()?;

let api = Api::new(api_key).configure(|cfg| {
    cfg.debug(DebugLevel::V);
    cfg.pagination(Caps::default().max_items(10_000));
});
```

Advanced runtime configuration also goes through `configure`:

```rust
let api = Api::new(api_key).configure(|cfg| {
    cfg.cache_store(cache);
    cfg.rate_limiter(limiter);
    cfg.transport(transport);
});
```

Facade navigation is primary:

```rust
let ids = riot
    .regional(region)
    .match_v5_matches()
    .ids_by_puuid(puuid)
    .count(100)
    .paginate()
    .max_items(10_000)
    .collect()
    .await?;
```

Required params are direct args. Optional/defaulted params are setters:

```rust
api.users().get(id).await?;
api.search().list().q("zed").page(2).clear_page().await?;
```

Session auth is explicit:

```rust
api.auth_api()
    .login_for_session(login)
    .acquire_as_session()
    .await?;

let me = api.protected().me().await?;
```

The explicit endpoint API remains advanced:

```rust
api.request(endpoints::users::GetUser::new(id))
    .execute()
    .await?;
```
