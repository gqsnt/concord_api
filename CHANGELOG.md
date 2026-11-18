# Changelog

This page lists what changed in each release of Concord. The five crates (`concord-api`, `concord-core`, `concord-macros`, `concord-extras`, `concord-testkit`) share one version and this single changelog.

## Unreleased

### Breaking changes

- **`concord::page::Slot` has a third required method, `input()`**, which says which input of the endpoint a slot names: a `concord::page::InputId`, its position among the inputs. The slots that `api!` generates implement it. A slot of your own that wraps another returns the `input()` of the slot it wraps (`fn input(&self) -> InputId { self.0.input() }`); one over a call type of your own numbers its inputs with `InputId::new(n)`. `or_when_set` knows an input by it, so two inputs that share storage or take no room are two inputs, and a slot that forwards to one is that one.
- **`concord::transfer::TransferError::EmptyQuery` is removed.** `transfer::at` accepts a URL whose query is a lone `?` or only `&`, and sends it as written, as `at_url` now does; a `match` arm that names the variant no longer compiles, and can be deleted.

## 5.0.0-rc.1

The first release candidate of Concord 5. It is not on crates.io yet: add it as a Git dependency (`concord-api = { git = "https://github.com/gqsnt/concord_api" }`).

Concord 5 turns one nested description of an HTTP API into a typed client built on `reqwest`. You write the API as a tree in `concord::api!`: a base, scopes, endpoints, and where each input goes on the wire. The macro generates a client type, one method for each endpoint, and one error type. The macro has no built-in format: `json`, `Json` and the pagers are ordinary Rust items, and your own can take their place. Retries are off until you turn them on, and never repeat a request that may not be repeated; a secret never appears in an error or a log; a call never waits forever on a silent server; and a test needs no network. See [Concord in ten minutes](docs/guide/tour.md).

- **Generated calls.** Each client has a `Call<E>`, where `E` is the endpoint's type. In type annotations and function signatures that name the call for `riot::matches::Ids`, write `riot::Call<riot::matches::Ids>`. Put inherent methods for that call in `impl riot::Call<riot::matches::Ids>`. In another crate, implement your trait for `riot::Call<riot::matches::Ids>`. A `Drop` implementation for one endpoint needs a wrapper around the call. Rename any scope or endpoint named `endpoint` directly under the client because its module reserves `Endpoint`. Use `E: riot::Endpoint` and `E::Value` to write one function for any call of the client. The endpoint type's page lists its options and what a send returns. The `Call` page lists the shared methods. See [Call a client](docs/guide/calling.md#the-public-api-of-a-client).

Optional features of `concord-api`: `gzip` and `deflate` make `reqwest`'s decoders available to `concord::Http`, which still decompresses only when `Http::with` turns it on, and `socks` makes SOCKS proxies available ([compression, proxies and TLS](docs/guide/calling.md#compression-proxies-and-tls)).

Beside the core, `concord` also ships four values, each built on the public traits of the core and each described in [the contracts](docs/reference/contracts.md#c11-a-credential-that-is-renewed-limits-records-a-returned-url), C11:

- `concord::auth::Renewed` (feature `auth`): a token that expires, renewed once for all the calls that wait for it and sent only to the origin of its base; and `concord::auth::after_401`, a retry rule for one more attempt after a `401`.
- `concord::limit::Limit` (feature `limit`): rate limits: a wait after a `429` with `Retry-After` (or, with `without_retry_after`, a wait that you choose after one without it), a cap on the calls in flight, and buckets with windows known in advance or learned from the headers of responses.
- `concord::read::JsonLines` and `concord::read::Sse`: readers of records into a `RecordStream`, each record held to a bound that has no default.
- `concord::transfer::at`: the call of a second description, sent to a URL that the API returned, once the URL is checked.

Two more values, and request body writers, are described in [the contracts](docs/reference/contracts.md#c12-body-writers-a-refresh-token-that-rotates-signing), C12:

- `concord::auth::Rotating` (feature `rotating`): a refresh token that the server replaces at each exchange. Each new pair is saved before either token is used, and a pair that the store could not keep is handed back to the application, never dropped. Its `RotationDriver` is a future that the application polls: Concord starts no task.
- `concord::sign` (feature `sign`): `SigV4`, AWS Signature Version 4, as a middleware that signs each attempt and as `SigV4::sign` for one request; `body_bytes` and `check_origin` for a provider's own proof.

- **Nested forms.** `bracket_form(value)` and `bracket_pairs(value)` (feature `form`) use bracketed maps and indexed lists.
- **Gzip requests.** `gzip(body)` (feature `gzip-body`) compresses buffered bytes and keeps their content type; the description sets the encoding header.
- **MIME multipart.** `multipart_mixed(parts)` and `multipart_related(parts)` (feature `multipart`) write buffered or repeatable parts with their own headers.
- **JSON records.** `json_lines(records, max_bytes)` (feature `json`) writes LF after every record and bounds the whole request body.

The core's `Body` has four items for body writers and large uploads, described in [the contracts](docs/reference/contracts.md#c12-body-writers-a-refresh-token-that-rotates-signing), C12.11 to C12.14:

- `Body::get_content_type()`: the content type that a body carries, whole, for a writer that wraps another body; `gzip` uses it.
- `Body::repeatable(make)`: a body whose stream is opened again, from its first byte, for each attempt, so that a large upload can be retried under the usual rules of retry. An edit sees a placeholder in place of such a body, and a failure of the body is the failure of its attempt. A `Body::stream` is still sent once.
- `Body::map_stream(map)`: a writer of your own around the stream of a body, run once for each opening; the multipart writers use it for a repeatable part.
- `Body::is_repeatable()`: whether a body can supply its stream again for another attempt, without opening anything.

The guide also has recipes that need no new item: a key made once per write, a documented download redirect, a large upload sent again with a new stream, binary frames, a multipart response and its batch, several hosts, and a provider's own HMAC proof.

### Coming from Concord 4

Concord 5 is a new design, not an upgrade in place: a description written for Concord 4 does not compile and must be rewritten. The package is now `concord-api`, used as `concord` in code. The main changes:

| Concord 4 | Concord 5 |
|---|---|
| Placement words in the signature | A plain signature; where each input goes is stated in the block |
| Four kinds of values, with a `?` mark | Two: required, or optional with a mark on the name (`count?: u32`) |
| `set_x`, `maybe_x` | `.x(v)` |
| Four keywords for exceptions | `override` and `remove` |
| `base` is fixed text, and may hold a path | A scheme, a host, an optional port and an optional path prefix; a placeholder in the host, or a whole base from an input of type `concord::Base` |
| No size limit unless one is set | 10 MiB by default |
| The caller brings the `reqwest` client | `concord::Http` by default; the caller may bring one with `with_transport` |
| An error exposes the unread `reqwest` response | A Refused error or a Mismatch keeps the status, the headers and at most 64 KiB of the body |
| A status with a meaning of its own needs a hand-written format | `status { }` arms in the description |
| No way to test without a socket | `with_transport(fake(...))`: a function that replaces the network |
| No logging | One `tracing` event, `retry`, with names and no values |
| Separate crates for auth, retry and pagination | `idempotent` and `paginate` in the description, `with_retry` on the client; a credential is a `Secret` input placed in `headers` or `query`; a signature is a `middleware`; a token that is renewed is `concord::auth::Renewed`, a middleware |
