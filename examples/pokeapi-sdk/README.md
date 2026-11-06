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

```rust,no_run
use concord::reqwest;
use concord_sdk_pokeapi::Client;

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let http = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .build()?;
let api = Client::new(http); // no credentials or context fields
let response = api.pokemon("clefairy").await?;
println!("{}: {}", response.value.id, response.value.name);
# Ok(()) }
```

List one page at a time, use `next` as returned, and fetch a selected result by
its URL. A server can return a different limit in the next URL; the caller does
not reconstruct it. To change the page size mid-traversal, call
`api.follow_with_limit(next, 50)` instead of `api.follow(next)`: it replaces only
the URL's `limit` pair (in place, dropping later duplicates, appended when
absent) and keeps the server's `offset` and every other pair byte for byte, in
order. An empty page is not itself completion: only `next: null` is.

```rust,no_run
# use concord::reqwest;
# use concord_sdk_pokeapi::Client;
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let api = Client::new(reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none()).build()?);
let mut page = api.list(Some(20), Some(0)).await?.value;
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

Each call reads and decodes the entire JSON response before returning. It is a
single page or detail, not a streaming interface. A successful page remains
owned by the caller if a subsequent request fails; the caller decides how to
store partial results, retry or stop. `Error::Concord` retains the original
Concord error, and `meta` contains status, headers and final URL when a head was
received, including on body/decode errors. A status or metadata error's unread
response can be recovered via `error.into_response()`; transport and build
failures have no response metadata. `Error::InvalidUrl` means no request was
sent for that URL. The provider documentation does not specify a standard
error body; this SDK does not claim one.

`follow`, `follow_with_limit` and `pokemon_url` require absolute same-origin URLs
within the configured Pokémon collection, and reject fragments and userinfo. The
HTTP transport's redirect policy is separate: configure it to prohibit
cross-origin redirects if that restriction matters. Query strings are passed
through URL parsing and are not regenerated from `limit` or `offset`;
`follow_with_limit` rewrites only `limit`. Applications should bound traversals
or detect repeated URLs if needed; this SDK does not detect cycles or retry.
