# concord-core

The core of Concord: the traits and the engine of the clients that `concord::api!` writes.
Concord is the crate `concord-api`, used as `concord` in code.

It knows no format, no scheme and no pager. It holds the traits that they are written with:
`Reader`, `Transport`, `Middleware`, the pager's `Call` and `Slot`, and the values `Body`,
`Secret`, `Base`, `Retry`, `Error`. The engine builds each request, sends it through the
middlewares and the transport, tries again when the policy says so, runs the two clocks,
reads the answer and walks the pages.

## Who depends on it

Most users: nobody. Depend on `concord-api`. It re-exports everything here that a user needs,
and it pins this crate and `concord-macros` to the same exact release.

Depend on `concord-core` directly only if you write a crate that adds a reader, a writer,
a pager, a middleware or a transport, and you do not want the defaults of `concord-api`
(its TLS stack, its proxy settings). This crate turns on no feature of `reqwest` by default.

Until the crates are on crates.io, depend on the repository:

```toml
[dependencies]
concord-core = { git = "https://github.com/gqsnt/concord_api" }
```

After the first upload, the line is `concord-core = "5.0.0-rc.1"`.

How to write each of those, with code to copy: [when Concord does not say what you need](https://github.com/gqsnt/concord_api/blob/main/docs/guide/extending.md).

| Feature | What it turns on | Default |
|---|---|---|
| `rustls` | TLS for `concord::Http`, through `reqwest/rustls` | off |
| `http2` | HTTP/2, through `reqwest/http2` | off |
| `system-proxy` | The proxy settings of the system, as `reqwest` reads them | off |

The hidden module `__private` is for the code that the macro writes. It is not public
interface, and it can change in any release.

Minimum Rust version: 1.96.
