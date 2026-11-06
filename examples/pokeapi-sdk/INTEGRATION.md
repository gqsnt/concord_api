# Post-wave-1 integration: one adapter for every operation (`concord::Endpoint`)

Coordinator record. The author's design (`2079f14b`) wrapped each generated operation in a
`Client` method so that callers got `Response { value, meta }` and the SDK's `Error`. Concord now
implements `concord::Endpoint<T>` for every generated endpoint, so one SDK trait can do that for
all of them. "Before" excerpts are from `1b8fd2b2` (the author's design plus the typed URL
reason), "after" from the commit that adds this file.

## Verdict: adopted

The generated client needs no configuration (`pub client PokeApi()`), so it can be the client the
caller holds: `pub type Client = PokeApi;` keeps the documented name with no wrapper type, no
accessor and no `Deref`. The two per-operation wrappers (`pokemon`, `list`) disappear, and the
list's optional values read as the generated setters instead of positional `Option`s. Provider
policy stays in the SDK: capturing the head before decoding (the adapter) and validating server
URLs (`follow`, `pokemon_url`, now inherent methods of `PokeApi`, sharing the adapter's tail).

What a caller pays: one more import (`Call`) and `.call()` on declared operations, while
`follow`/`pokemon_url` stay plain `async fn`s; the generated `send()` (value only, Concord's
error) is visible next to `call()`, as it already was in the Nager SDK.

## Declaration

Unchanged; only its doc comment now documents the client (before: "Low-level generated
operations. Prefer [`Client`] for metadata and URL following.").

```rust
        GET pokemon(id: String) -> Pokemon { path "pokemon/{id}/"; }
        /// Request one list page.
        GET list(query limit?: u64, query offset?: u64) -> PokemonPage { path "pokemon/"; }
```

## SDK adapter

Before (`src/lib.rs` at `1b8fd2b2`), a wrapper type and one method per operation:

```rust
#[derive(Clone)]
pub struct Client {
    api: PokeApi,
}
```

```rust
    pub async fn pokemon(&self, id: impl Into<String>) -> Result<Response<Pokemon>, Error> {
        send(self.api.pokemon(id.into()).prepare()?, None).await
    }
```

```rust
    pub async fn list(
        &self,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> Result<Response<PokemonPage>, Error> {
        let mut op = self.api.list();
        if let Some(limit) = limit {
            op = op.set_limit(limit);
        }
        if let Some(offset) = offset {
            op = op.set_offset(offset);
        }
        send(op.prepare()?, None).await
    }
```

```rust
async fn send<T>(prepared: Prepared<T>, url: Option<Url>) -> Result<Response<T>, Error> {
    let (client, mut request, decoder) = prepared.into_parts();
    if let Some(url) = url {
        *request.url_mut() = url;
    }
    let received = decoder.execute(&client, request).await?;
```

After (`src/lib.rs`, doc comments omitted), one trait for every operation:

```rust
pub type Client = PokeApi;
```

```rust
pub trait Call<T>: Endpoint<T> {
```

```rust
    fn call(self) -> impl Future<Output = Result<Response<T>, Error>> + Send;
}

impl<T: Send, E: Endpoint<T> + Send> Call<T> for E {
    async fn call(self) -> Result<Response<T>, Error> {
        finish(self.prepare()?.receive().await?).await
    }
}
```

and the URL-following methods execute the edited request with the declared decoder, then share
the same `finish(received)` (head capture, then decode):

```rust
    pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
        let (client, mut request, decoder) = self.list().prepare()?.into_parts();
        *request.url_mut() = checked_url(request.url(), next, false)?;
        finish(decoder.execute(&client, request).await?).await
    }
```

## One caller call

Before (`tests/workflows.rs`):

```rust
    let first = api.list(Some(2), Some(0)).await.unwrap().value;
```

After (as `rustfmt` lays it out: the chain exceeds its default `chain_width`):

```rust
    let first = api
        .list()
        .set_limit(2)
        .set_offset(0)
        .call()
        .await
        .unwrap()
        .value;
```

The named setters say which number is which; the cost is a longer chain. Imports go from
`use concord_sdk_pokeapi::Client;` to `use concord_sdk_pokeapi::{Call, Client};`.

## Adding an operation (say `GET pokemon_species(id: String) -> Species`)

| Before | After |
|---|---|
| 1. the `api!` entry | 1. the `api!` entry |
| 2. a `Client` method: argument mapping (`impl Into<String>`, or `Option`s to setters), `send(self.api.op(..).prepare()?, None)`, its doc comment | — (`Call` serves it: `api.pokemon_species("x").call()`) |
| 3. README | 2. README |

A method of the SDK is still needed for a URL the server returns (as `follow`/`pokemon_url`),
because that is a provider rule, not a declared operation.

## What changed for callers and tests

- `Client` is the generated type: its `Debug` is the generated one (`PokeApi { base: .. }`), and
  `with_base_url` returns `concord::Error` (always a `Build` error) instead of
  `Error::Concord { meta: None, .. }`.
- `api.pokemon(id).await` becomes `api.pokemon(id).call().await`; `api.list(limit, offset)`
  becomes `api.list()` with `set_limit`/`set_offset` (or `maybe_limit`/`maybe_offset` for an
  `Option`) and `.call()`.
- The `call()` future is `Send` (bound in the trait), so it can be spawned.
- Tests: 21 call sites in `tests/{first_use,protocol,workflows}.rs` (rustfmt wraps some over
  several lines) and one import line per file; no expectation changed. The README examples are
  now compiled as doctests (`ReadmeDoctests` in `src/lib.rs`).
