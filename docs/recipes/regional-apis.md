# Regional APIs

Model a regional API with a scope value when the host, path, and public
headers vary together:

```rust
api! {
    client RegionalApi {
        base "https://example.test/";
    }

    scope region(region: String) {
        host [scope.region, "api"];
        path ["regions", scope.region];
        headers { "X-Region" = scope.region; }

        GET status() -> Json<Status>;
    }
}
```

The generated use reads like normal nesting:

```rust
let request = client.region("eu-west".to_owned()).status();
```

Host labels are validated and encoded separately from path segments. Only
public values may shape the URL or public headers; put secret authentication
in credential declarations and `auth` placements.

If routing changes dynamically after compilation, build a
`concord::endpoint::RuntimeEndpoint` with a bounded `EndpointId`, but retain a
typed response specification and the same managed executor. Do not create one
client per request merely to change a public scope value.

The importable
[regional scope example](../../crates/concord-examples/src/contracts/regional_scopes.rs)
is checked by the
[construction test](../../crates/concord-examples/tests/construction.rs).
The split [Riot recipe](riot.md) shows a larger routing-domain model.
