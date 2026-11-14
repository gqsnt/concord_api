# concord-extras

What Concord ships alongside its core. Concord is the crate `concord-api`, used as `concord` in code.

Its readers, writers and
pagers are built on the core's public traits and helpers, including `body::Bounded`,
`read::details` and `page::fragmentless`, and your own reader, writer or pager uses
the same items. Only the `tower` adapter also uses a hidden helper of the engine, for
the URL of a response.

| What | Items |
|---|---|
| Body writers | `json(x)`, `form(x)`, `multipart()` (feature `multipart`) |
| Readers | `Json`, `Text`, `Binary`, `Stream` |
| Pagers | `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link`, `by_next_url` |
| A credential | `basic(user, password)`, which returns a `Secret` |
| A transport | `tower::Tower`, a `tower` service as a transport (feature `tower`) |

## Who depends on it

For most users, nothing: depend on `concord-api` instead, which re-exports all of this and enables
these features through its own (`json`, `form`, `multipart`, `tower`).

Read its source when you write your own reader, writer or pager: every file except
`tower.rs` is an example of the public traits and helpers. The guide covers each one, with code
to copy, in [When Concord does not cover what you need](https://github.com/gqsnt/concord_api/blob/main/docs/guide/extending.md).

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)` and `Json` | on |
| `form` | `form(x)` | on |
| `multipart` | `multipart()` | off |
| `tower` | `tower::Tower` | off |

Minimum Rust version: 1.96.
