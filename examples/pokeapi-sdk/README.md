# PokéAPI Pokémon SDK

This unpublished example covers PokéAPI v2 `GET pokemon/{id or name}/` and the
Pokémon resource list (`limit` and `offset`). GraphQL, other resource families,
encounters and automatic traversal are out of scope. A `Pokemon` has typed
scalar fields; other fields, including nested resources and sprites, are retained
as JSON in `extra`. `Resource`, `PokemonPage`, `Response` and `Error` are typed.
Fixtures are synthetic offline examples, not vendor conformance tests. No live
provider calls were made.

PokéAPI requires no authentication. Supply a Concord-reexported reqwest client to
configure transport, timeouts and redirect policy. The default base is
`https://pokeapi.co/api/v2`; `with_base_url` overrides it for local testing. The
provider asks applications to cache requested resources; this SDK does not cache.

`Client` is the generated `PokeApi` client (the name diagnostics and operation
names use: `PokeApi.pokemon`, `PokeApi.list`). A declared operation is sent with
its `.send()`, which goes through the SDK's send policy `KeepHead` (`send KeepHead;`
in the declaration) and returns the value with the response head
(`Response { value, meta }`) or an `Error` that keeps the head. Nothing to import
besides `Client`. `.prepare()` builds the native request with Concord's own error.

```rust,no_run
use concord::reqwest;
use concord_sdk_pokeapi::Client;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
let api = Client::new(http); // no credentials or context fields
let response = api.pokemon("clefairy").send().await?;
println!("{}: {}", response.value.id, response.value.name);
# Ok(()) }
```

List one page at a time, use `next` as returned, and fetch a selected result by
its URL. A server can return a different limit in the next URL; the caller does
not reconstruct it. An empty page is not itself completion: only `next: null` is.
`limit` and `offset` are optional; an omitted one uses the provider's default.

```rust,no_run
# use concord::reqwest;
# use concord_sdk_pokeapi::Client;
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let api = Client::new(reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none()).build()?);
let mut page = api.list().set_limit(20).set_offset(0).send().await?.value;
loop {
    if let Some(selected) = page.results.first() {
        let pokemon = api.pokemon_url(&selected.url).await?.value;
        println!("{}", pokemon.name);
    }
    match page.next.as_deref() {
        Some(next) => page = api.follow(next).await?.value,
        None => break,
    }
}
# Ok(()) }
```

Each `send()`, `follow` and `pokemon_url` reads and decodes the entire JSON
response before returning. It is a single page or detail, not a streaming
interface. A successful page remains
owned by the caller if a subsequent request fails; the caller decides how to
store partial results, retry or stop. `Error::Concord` retains the original
Concord error, and `meta` contains status, headers and final URL when a head was
received, including on body/decode errors. A status or metadata error's unread
response can be recovered via `error.into_response()`; transport and build
failures have no response metadata. `Error::InvalidUrl(reason)` means no request
was sent for that URL; the `UrlRejection` reason says which check failed
(unparseable, foreign origin, user info, fragment, wrong path, or a query on a
detail URL), and neither `Display` nor `Debug` shows the URL. The provider
documentation does not specify a standard error body; this SDK does not claim one.

`follow` and `pokemon_url` require absolute same-origin URLs within the configured
Pokémon collection, and reject fragments and userinfo. The HTTP transport's
redirect policy is separate: configure it to prohibit cross-origin redirects if
that restriction matters. Query strings are passed through URL parsing and are
not regenerated from `limit` or `offset`. Applications should bound traversals
or detect repeated URLs if needed; this SDK does not detect cycles or retry.
