# concord-core

The core of Concord: the traits, and the engine of the clients that `concord::api!` generates.
Concord is the crate `concord-api`, used as `concord` in code.

It has no built-in format, authentication scheme or pager. It holds the traits that they are built on:
`Reader`, `Transport`, `Middleware`, the pager's `Call` and `Slot`, and the values `Body`,
`Secret`, `Base`, `Retry`, `Error`. The engine builds each request, sends it through the
middleware and the transport, retries when the policy says so, enforces the two timeouts,
reads the response and walks through the pages.

## Who depends on it

For most users, nothing: depend on `concord-api` instead. It re-exports everything here that a
user needs, and it pins this crate and `concord-macros` to the same exact release.

Depend on `concord-core` directly only if you write a crate that adds a reader, a writer,
a pager, a middleware or a transport, and you do not want the defaults of `concord-api`
(its TLS stack, its proxy settings). This crate enables no feature of `reqwest` by default.

Until the crates are on crates.io, add it as a Git dependency:

```toml
[dependencies]
concord-core = { git = "https://github.com/gqsnt/concord_api" }
```

Once the crates are published on crates.io, the line becomes `concord-core = "5.0.0-rc.1"`.

See [When Concord does not cover what you need](https://github.com/gqsnt/concord_api/blob/main/docs/guide/extending.md) for how to write each of these, with code to copy.

| Feature | What it turns on | Default |
|---|---|---|
| `rustls` | TLS for `concord::Http`, through `reqwest/rustls` | off |
| `http2` | HTTP/2, through `reqwest/http2` | off |
| `system-proxy` | The system's proxy settings, as `reqwest` reads them | off |

The hidden module `__private` exists for the code that the macro generates. It is not part of
the public interface, and it can change in any release.

Minimum Rust version: 1.96.
