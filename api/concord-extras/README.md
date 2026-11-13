# concord-extras

What Concord (the crate `concord`) ships beside its core. Readers, writers and
pagers use the core's public traits and helpers, including `body::Bounded`,
`read::details` and `page::fragmentless`; extensions supplied by an author use the
same doors. The Tower adapter also uses version-pinned internal engine support
for response URLs.

| What | Items |
|---|---|
| Body writers | `json(x)`, `form(x)`, `multipart()` (feature `multipart`) |
| Readers | `Json`, `Text`, `Binary`, `Stream` |
| Pagers | `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link`, `by_next_url` |
| A credential | `basic(user, password)`, which gives a `Secret` |
| A transport | `tower::Tower`, a `tower` service as a transport (feature `tower`) |

## Who depends on it

Most users: nobody. Depend on `concord`, which re-exports all of this and turns the features
on through its own (`json`, `form`, `multipart`, `tower`).

Read its source when you write a reader, a writer or a pager of your own: each file but
`tower.rs` is an example of the public traits and helpers.

| Feature | What it turns on | Default |
|---|---|---|
| `json` | `json(x)` and `Json` | on |
| `form` | `form(x)` | on |
| `multipart` | `multipart()` | off |
| `tower` | `tower::Tower` | off |

Minimum Rust version: 1.96.
