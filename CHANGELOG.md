# Changelog

This page lists what changed in each release of Concord. The five crates (`concord-api`, `concord-core`, `concord-macros`, `concord-extras`, `concord-testkit`) share one version and this single changelog.

## 5.0.0-rc.1

The first release candidate of Concord 5. It is not on crates.io yet: add it as a Git dependency (`concord-api = { git = "https://github.com/gqsnt/concord_api" }`).

Concord 5 turns one nested description of an HTTP API into a typed client built on `reqwest`. You write the API as a tree in `concord::api!`: a base, scopes, endpoints, and where each input goes on the wire. The macro generates a client type, one method for each endpoint, and one error type. The macro has no built-in format: `json`, `Json` and the pagers are ordinary Rust items, and your own can take their place. Retries are off until you turn them on, and never repeat a request that may not be repeated; a secret never appears in an error or a log; a call never waits forever on a silent server; and a test needs no network. See [Concord in ten minutes](docs/guide/tour.md).

Optional features of `concord-api`: `gzip` and `deflate` make `reqwest`'s decoders available to `concord::Http`, which still decompresses only when `Http::with` turns it on, and `socks` makes SOCKS proxies available ([compression, proxies and TLS](docs/guide/calling.md#compression-proxies-and-tls)).

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
| Separate crates for auth, retry and pagination | `idempotent` and `paginate` in the description, `with_retry` on the client; a credential is a `Secret` input placed in `headers` or `query`; a signature is a `middleware` |
