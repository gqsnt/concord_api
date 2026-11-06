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
- The trait states `+ Send`, so code generic over `E: Call<T>` can spawn the future. A concrete
  call (`tokio::spawn(async move { api.pokemon("x").call().await })`) compiles without the bound
  too, because the auto trait leaks through the one blanket impl (checked with a temporary
  probe test, then removed).
- Tests: 21 call sites in `tests/{first_use,protocol,workflows}.rs` (rustfmt wraps some over
  several lines) and one import line per file; no expectation changed. The README examples are
  now compiled as doctests (`ReadmeDoctests` in `src/lib.rs`).

# Between waves 2-3

Coordinator integration of two library refinements: a client **send policy** (`send P;`, every
generated `send()` returns `<P as concord::SendWith<T>>::Output`) and **`Prepared::follow(url)`**
(the same endpoint request at a server-returned, same-origin URL). "Before" is `1f71432c` (the
`Call` adapter recorded above), "after" is the commit that adds this section.

## Verdict: adopted (send policy and `Prepared::follow`)

The send policy is the `Call` adapter without its cost to callers: the same one-line body now
runs behind the generated `send()`, so callers write `api.pokemon(id).send()` with only `Client`
imported, and there is one terminal method per operation instead of two (`call()` returning the
SDK's `Response`/`Error` next to a generated `send()` returning Concord's error). `follow` and
`pokemon_url` keep the SDK's checks and typed `UrlRejection`, then replace the URL with
`Prepared::follow` instead of `into_parts` → `url_mut` → `Decoder::execute`.

## Declaration

Before (`src/lib.rs`):

```rust
    pub client PokeApi() {
        base "https://pokeapi.co/api/v2";
        format Json;
```

After:

```rust
    pub client PokeApi() {
        base "https://pokeapi.co/api/v2";
        send KeepHead;
        format Json;
```

The two operations are unchanged.

## SDK adapter

Before, the trait every caller imported:

```rust
pub trait Call<T>: Endpoint<T> {
    fn call(self) -> impl Future<Output = Result<Response<T>, Error>> + Send;
}

impl<T: Send, E: Endpoint<T> + Send> Call<T> for E {
    async fn call(self) -> Result<Response<T>, Error> {
        finish(self.prepare()?.receive().await?).await
    }
}
```

After, the policy the declaration names (doc comment omitted):

```rust
pub struct KeepHead;

impl<T: Send> SendWith<T> for KeepHead {
    type Output = Result<Response<T>, Error>;

    async fn send<E: Endpoint<T> + Send>(endpoint: E) -> Self::Output {
        finish(endpoint.prepare()?.receive().await?).await
    }
}
```

URL following, before:

```rust
    pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
        let (client, mut request, decoder) = self.list().prepare()?.into_parts();
        *request.url_mut() = checked_url(request.url(), next, false)?;
        finish(decoder.execute(&client, request).await?).await
    }
```

```rust
        let list = self.list().prepare()?;
        let url = checked_url(list.request().url(), url, true)?;
        // The `pokemon` operation's decoder and identity; its placeholder URL is replaced.
        let (client, mut request, decoder) = self.pokemon("placeholder").prepare()?.into_parts();
        *request.url_mut() = url;
        finish(decoder.execute(&client, request).await?).await
```

After (`checked_url` now only validates, `-> Result<(), Error>`; `Prepared::follow` parses the
same string and replaces the URL verbatim):

```rust
    pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
        let list = self.list().prepare()?;
        checked_url(list.request().url(), next, false)?;
        finish(list.follow(next)?.receive().await?).await
    }
```

```rust
        checked_url(self.list().prepare()?.request().url(), url, true)?;
        // The `pokemon` operation's request (decoder, identity) at the checked URL.
        let detail = self.pokemon("placeholder").prepare()?.follow(url)?;
        finish(detail.receive().await?).await
```

The SDK's checks still run first and report the typed reason (`ForeignOrigin`, `Userinfo`,
`Fragment`, `WrongPath`, `Query`, `Unparseable`) with nothing sent; `Prepared::follow`'s own
same-origin refusal (a Concord `Build` error) is therefore unreachable here and stays as a second
line of defence. The followed request keeps the endpoint's method, headers, decoder and identity
(`PokeApi.list` / `PokeApi.pokemon` in errors), as before.

## One caller call

Before (`tests/workflows.rs`):

```rust
use concord_sdk_pokeapi::{Call, Client};
```

```rust
    let pokemon = api(&server).pokemon("clefairy").call().await.unwrap().value;
```

After:

```rust
use concord_sdk_pokeapi::Client;
```

```rust
    let pokemon = api(&server).pokemon("clefairy").send().await.unwrap().value;
```

## Lines of SDK adapter code

Measured on `src/lib.rs` (adapter type/trait and its impl, `impl PokeApi` with `follow` and
`pokemon_url`, `checked_url`, `finish`); "code" excludes blank and comment lines.

| | lines | code |
|---|---|---|
| before (`Call` trait + impl) | 89 | 68 |
| after (`KeepHead` + impl) | 88 | 65 |

The adapter is the same size; the change is on the caller's side (no import, one `send()`) and
in `follow`/`pokemon_url` (no native parts).

## Adding an operation (say `GET pokemon_species(id: String) -> Species`)

| Before | After |
|---|---|
| 1. the `api!` entry | 1. the `api!` entry |
| — (`Call` served it; callers needed the import) | — (`KeepHead` serves it through the generated `send()`) |
| 2. README | 2. README |

A new output type needs nothing more: the policy is generic (`impl<T: Send> SendWith<T>`). A
policy written for fixed outputs makes a new output a compile error naming the policy instead
(observed in the Nager SDK's record, which uses one).

## What a caller sees (observed)

- Rustdoc of a generated `send()` (`PokeApiPokemon::send`) reads
  `pub fn send(self) -> impl Future<Output = <KeepHead as SendWith<Pokemon>>::Output> + Send`
  with the generated sentence "Sends the request with the client's send policy `KeepHead` ...";
  the concrete `Result<Response<Pokemon>, Error>` is one click away, on `KeepHead`. The
  `Call::call` signature showed it directly. This is the cost of the change.
- rustc normalises the projection in diagnostics. A temporary probe (compiled, then removed)
  `let pokemon: Pokemon = api.pokemon("clefairy").send().await?;` reported
  `error[E0308]: ... expected 'Pokemon', found 'Response<Pokemon>'`.
- Passing the operation itself where a future is expected
  (`tokio::time::timeout(d, api.pokemon("clefairy"))`) is still `error[E0277]: 'PokeApiPokemon'
  is not a future`, with no hint towards `.send()` (and a misleading "remove the `.await`"); the
  same as with `.call()` before.
- `prepare()` is unchanged (Concord's `Prepared` and error).

## Tests

21 call sites `.call()` → `.send()` and the import line of each of the three test files
(`Call` removed); no expectation changed. README: the caller paragraph and two examples (compiled
as doctests). Results: `first_use` 14, `protocol` 6, `workflows` 3, doctests 2 — all pass, as at
`1f71432c`; `cargo clippy --all-targets -D warnings` and `cargo doc` (`-D warnings`) clean.

`Received::decode_limited` was not adopted: PokéAPI documents no size for a Pokémon document
(it embeds every move and sprite), and an invented bound would turn valid responses into errors.
