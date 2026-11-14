# concord-extras

What Concord ships beside its core. Concord is the crate `concord-api`, used as `concord` in code.

Readers, writers and
pagers use the core's public traits and helpers, including `body::Bounded`,
`read::details` and `page::fragmentless`; a reader, a writer or a pager of yours uses
the same items. Only the `tower` adapter also uses a hidden helper of the engine, for
the URL of an answer.

| What | Items |
|---|---|
| Body writers | `json(x)`, `form(x)`, `multipart()` (feature `multipart`) |
| Readers | `Json`, `Text`, `Binary`, `Stream` |
| Pagers | `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link`, `by_next_url` |
| A credential | `basic(user, password)`, which gives a `Secret` |
| A transport | `tower::Tower`, a `tower` service as a transport (feature `tower`) |

## Who depends on it

Most users: nobody. Depend on `concord-api`, which re-exports all of this and turns the features
on through its own (`json`, `form`, `multipart`, `tower`).

Read its source when you write a reader, a writer or a pager of your own: each file but
`tower.rs` is an example of the public traits and helpers. The guide has each one with code
to copy: [when Concord does not say what you need](https://github.com/gqsnt/concord_api/blob/main/docs/guide/extending.md).

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)` and `Json` | on |
| `form` | `form(x)` | on |
| `multipart` | `multipart()` | off |
| `tower` | `tower::Tower` | off |

Minimum Rust version: 1.96.
