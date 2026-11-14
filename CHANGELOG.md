# Changelog

For users of Concord: what changed in each release. The five crates (`concord-api`, `concord-core`, `concord-macros`, `concord-extras`, `concord-testkit`) share one version, and this one changelog.

## 5.0.0-rc.1

The first release candidate of Concord 5. It is not on crates.io yet: depend on the repository (`concord-api = { git = "https://github.com/gqsnt/concord_api" }`).

Concord 5 turns one nested description of an HTTP API into a typed client over `reqwest`. You write the API as a tree in `concord::api!`: a base, scopes, endpoints, and where each input goes on the wire. The macro writes a client type, one method for each endpoint, and one error type. The macro knows no format: `json`, `Json` and the pagers are ordinary Rust items, and one of yours takes the same place. Retries are off until set, and never repeat a request that may not be repeated; a secret never appears in an error or a log; a call never waits for ever on a silent server; and a test needs no network. See [Concord in ten minutes](docs/guide/tour.md).

### Coming from Concord 4

Concord 5 is a new design, not an upgrade in place: a description written for Concord 4 does not compile, and is written again. The package is now `concord-api`, used as `concord` in code. What changes, in short:

| Concord 4 | Concord 5 |
|---|---|
| Place words in the signature | A plain signature; where each input goes is said in the block |
| Four kinds of values, with a `?` mark | Two: required, or optional with a mark on the name (`count?: u32`) |
| `set_x`, `maybe_x` | `.x(v)` |
| Four words for exceptions | `override` and `remove` |
| `base` is fixed text, and may hold a path | A scheme, a host, maybe a port and a path prefix; a hole in the host, or a whole base from an input of type `concord::Base` |
| No size limit unless written | 10 MiB by default |
| The caller brings the `reqwest` client | `concord::Http` by default; the caller may bring one with `with_transport` |
| An error hands out the unread `reqwest` response | A Refused error or a Mismatch keeps the status, the headers and at most 64 KiB of the body |
| A status that means something needs a hand-written format | `status { }` arms in the description |
| No way to test without a socket | `with_transport(fake(...))`: a function in place of the network |
| No logging | One `tracing` event, `retry`, with names and no values |
| Separate crates for auth, retry and pagination | `idempotent` and `paginate` in the description, `with_retry` on the client; a credential is a `Secret` input placed in `headers` or `query`; a signature is a `middleware` |
