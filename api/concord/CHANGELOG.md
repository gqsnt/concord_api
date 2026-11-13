# Changelog

## 5.0.0-rc.1

First release of the V5 design.

### Coming from 4.x

V5 is a new design, not an upgrade in place: a description of 4.x does not compile, and is
written again. What changes, in short:

| 4.x | 5.0 |
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

The full list is in the design, `docs/9-background.md`, section 9.2.
