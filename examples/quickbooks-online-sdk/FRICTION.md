# Friction

What this SDK cost against the frozen baseline (`14b9e1e0`), with the evidence observed, what was
written by hand, which part is QuickBooks semantics and which generic mechanics, and the smallest
Concord change that would remove it. No shared crate was edited.

Line references are to this package after `cargo fmt`.

## 1. A send policy exists only on the client, so the realm is a client value

**Evidence.** The realm is the natural scope (`/v3/company/{realmId}/..`, every request), and
its access token must come from the same place. A probe declaration with the realm as a scope
carrying its own policy (`scope company(connection: Connection) { send connection -> Result<_, Error>; .. }`,
compiled once as a temporary test file and deleted) is refused:
`` `send` is only allowed on the client: one send policy serves every endpoint ``. A policy
on the client cannot read scope values either: `SendWith::send` receives `E: Endpoint<T>`, whose
only accessors are `prepare()` and `operation()`, and `reqwest::Request` carries no extensions.

**What was written.** The design puts the realm-bound `Connection` in the client
(`pub client QuickBooks(connection: Connection)`), names it as the policy (`send connection`)
**and** places it in the path (`path "v3/company/{connection}"`, its `Display` is the realm ID).
The path's realm and the request's token are then one value; `set_connection` replaces both
together. The cost: one `QuickBooks` client per connected company (cheap, but a multi-company
app holds a map of clients rather than one client and a `company(realm)` scope), and the realm
segment is withheld from `Prepared`'s `Debug` like any send-policy placement although a realm ID
is not a secret.

Counterexamples considered: a realm scope plus a client policy holding a realm→token map would
have to parse the realm back out of the built URL (fragile, and silently wrong under a
`with_base_url` whose path contains `company/`); a `realm` client value next to a `connection`
client value would let `set_realm(other)` send one realm's token to another realm's path, since
every value has a generated setter.

**Provider vs generic.** "One token per realm" is QuickBooks semantics; carrying a policy value
with the scope that owns it is generic mechanics.

**Smallest change.** Allow `send value;` on a scope whose value it names (the nearest policy
wins, like formats), or hand the policy the scope values it was declared to see. Either keeps
client → scope → endpoint and makes `qbo.company(connection).customers()..` expressible.

## 2. The policy reads the create's `requestid` back from the built URL

**Evidence.** `src/policy.rs:77-81`: `request.url().query_pairs().find(|(k, _)| k == "requestid")`
re-parses the value Concord itself placed from the declared `request_id: RequestId`, so that an
uncertain create can report it.

**Provider vs generic.** Which value identifies a write is QuickBooks' (D6); getting a declared
value back inside a policy is generic. Same root as item 1.

**Smallest change.** None needed for this slice (5 lines); a typed accessor from `Operation` or
`Endpoint` to declared values would remove it.

## 3. `concord-auth`'s token cache, evaluated against the realm and refresh-token rules

Read `crates/concord-auth/README.md` and `src/cache.rs`. Used for: one bounded cache per
connection keyed by its realm (`TokenCache<RealmId, TokenFailure>`, capacity 1, 60 s margin),
single-flight refresh (tested: `workflows.rs::concurrent_calls_share_one_refresh`, two reads,
one token request while both wait), generation-safe invalidation of exactly the token a 401
answered (`discard`, tested by the expired-access-token workflow), checked lifetimes, `bearer`
and `basic` credentials. What it could not express, each written in `src/auth.rs`:

| Gap | Evidence | Hand-written |
|---|---|---|
| A refresh that **rotates the refresh token**: acquisition returns only `Token { header, lifetime }` | `Token::new(header, lifetime)` has no slot for a second secret | `Grants` (`auth.rs:519-534`) holds the refresh token; `acquire` (`auth.rs:697-746`) writes the rotated token into it as a side effect before returning the access token |
| A **rejected grant must not be retried by every call**: a failed acquisition removes the entry, so the next `get_or_refresh` asks again | `cache.rs`: `Err(_) => { entries.remove(&key); }` | `Rejection` memo (`auth.rs:512-517`) returned by `acquire` without I/O until `reauthorize`/`replace_refresh_token` clears it; tested (`token_endpoint_refusals_are_classified`: terminal cases make no second token request) |
| **Seeding** the access token a code exchange already returned: there is no insert | no `insert`/`put` in the public API | `Grants::seed`, taken by the first acquisition with its remaining lifetime |
| A **replacement during a refresh flight** must win over the flight's result | `remove` stops the flight from caching its token, but the flight's *refresh token* lives outside the cache | `Grants::epoch`, compared when the flight finishes |
| The cache must not be captured by its own acquisition (README: an `Arc` cycle) | README, "Do not capture the cache in its own acquisition closure" | `Connection` state split into the cache and a separate `Arc<Mutex<Grants>>` that the closure captures |
| Refresh-token expiry (`x_refresh_token_expires_in`) | not representable in `Token` | `Grants::refresh_expires_at` |
| Binding a snapshot to **the realm of the request** it is applied to | the key is opaque by design | item 1: the token and the path come from one `Connection` |

**Provider vs generic.** Rotation, "store the latest", and when a refusal is terminal are
provider/OAuth semantics and belong in the SDK. Two pieces are generic: an acquisition result
that carries one more owned value back to the caller (here the rotated refresh token), and a
negative entry ("this key failed terminally, do not acquire again until told").

**Smallest change.** `TokenCache::insert(key, Token)` for an already-issued token, and either a
generic payload on `Token` returned beside the snapshot or a documented recipe for the rotation
side channel. A negative cache is a policy decision; a documented recipe would do.

## 4. A typed fault inside a 2xx crosses the format boundary as an erased error source

**Evidence.** QuickBooks documents a 200 that carries a `Fault` (D4) and a query error as a
`Fault` inside `QueryResponse` (D8). The format (`src/format.rs`, `QboJson`: `Json` plus a
probe) can only fail `decode_body` with a `CodecError` whose label is static, so the typed fault
travels as the error's source (`with_source(FaultInSuccess(fault))`) and the policy recovers it
with `error.codec_error()?.source_unredacted()?.downcast_ref::<FaultInSuccess>()` and clones it
(`policy.rs:154-161`). `Received::decode()` does not keep the body (`body_unredacted` is set only
by `document`), so re-parsing the body in the policy was not an option.

**Provider vs generic.** The fault rule is QuickBooks'; moving a typed refusal from a decoder to
its caller by value is generic.

**Smallest change.** A by-value way to take a codec error's source
(`Error::into_codec_source_unredacted() -> Option<Box<dyn Error + Send + Sync>>`), or keep the
body on a `decode()` `Decode` failure as `document()` already does.

## 5. `prepare()` is the undecorated request

**Evidence.** By Concord's design (author guide §5.13), `qbo.customer(id).read().prepare()?` has
no `Authorization`: only `send()` goes through the connection's policy
(`protocol.rs::each_environment_has_its_documented_base_and_prepare_adds_no_credential`). A
caller who sends a prepared request natively gets a 401. Documented on the client and in the
README; no change proposed.

## 6. Small observations

- The generated rustdoc of each endpoint type states the concrete signature
  `send(self) -> impl Future<Output = Result<CustomerResponse, Error>>`, while the struct's
  prose still says "`send()` returns its `SendWith<T>::Output` for this output `T`"
  (`target/doc/.../struct.QuickBooksCustomerRead.html`); with a declared `-> Result<_, Error>`
  template the prose could name the result too.
- `concord::Decoder<T>` is `Copy` (clippy `clone_on_copy` on `decoder.clone()`); harmless, but a
  reader of the concord-auth recipe would not expect it.
- The token endpoint is a second, `pub(crate)` `api!` client rebuilt for each token request
  (`TokenEndpoint::new(..).with_base_url(..)`, `auth.rs:436-477`) because a fixture base URL must
  be configurable on the app; cheap, no change proposed.
- A fresh `requestid` needs a random source; neither Concord nor the standard library offers
  one publicly, so `RequestId::generate` hashes a counter, the clock and the process ID with the
  standard library's randomly keyed hasher. SDK-owned; callers can supply their own ID.

## Shared capabilities not used, and why

- **`concord-retry`**: the only repetition in the slice is one read after a 401 with a new
  token, which the concord-auth README's ordinary `SendWith` recipe covers in a few lines; the
  retry README asks for **one** owner of repetition, and creates are never repeated. Read
  `crates/concord-retry/README.md`.
- **`concord-ratelimit`**: the cited pages give no throttle numbers to admit against (D4 lists a
  429 only in a change-order table); a 429 is returned as `Error::Provider`. Read its README.
- **`concord-codecs`**: used only through `concord`'s `Json` and `Form` (feature `form`). Read its
  README.
- **`concord-pagination`**: used. `Pages` for `customer_pages`, `next_offset(start, limit, len, None)`
  for the one-based `STARTPOSITION` rule; no friction beyond the SDK's own echo check.

## Examples consulted (example-assisted result)

| File | What was taken |
|---|---|
| `examples/zendesk-support-sdk/Cargo.toml`, `src/lib.rs`, `src/policy.rs`, `src/error.rs` | package layout; `send policy -> Result<_, Error>` with method-based read/write split; the uncertain-write model (`UncertainWrite`, `UncertainCause`, replay key); `http_client()` without retries or redirects; `ProviderError`-style head + document error; README doctest include |
| `examples/zendesk-support-sdk/tests/support/mod.rs` (lines 1–80), `PROVENANCE.md` (lines 1–60) | test helper shape (`fixture`, `json_fixture`, client wiring) and the provenance table format |
| `examples/hubspot-crm-sdk/src/lib.rs`, `Cargo.toml` | OAuth refresh with `TokenCache` and a refresh-state mutex; a `Form` token endpoint declaration (`concord` feature `form`). Not taken: its acquisition closure captures the struct that owns the cache (the cycle the README warns about) |
| `examples/slack-sdk/src/wire.rs` (lines 1–60) | how another SDK models an error inside a 2xx (an envelope type behind a private declaration); this SDK uses a format instead, keeping the declaration public |
| `examples/distribution-registry-sdk/src/api.rs` (lines 100–140) and a grep of `src/client.rs` | a `pub(crate) client` token-service declaration and the `validate_header_names()` unit test |
| `examples/testserver/src/lib.rs` | the loopback server API (`TestServer`, `Reply`, `Body`, `Gate`, `generated_records`) |
