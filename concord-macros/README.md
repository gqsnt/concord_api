# concord-macros

Concord's macro, `api!`. Concord is the crate `concord-api`, used as `concord` in code.

The macro reads a nested description
of an HTTP API and generates a typed client: a client type, one method for each endpoint, one
struct for each call, and one error type.

The macro handles the tree, the statements and the inputs. It has no built-in format, authentication
scheme or pager: what follows `read`, `paginate` or `body` is a Rust expression, and the
compiler checks it against a trait of the core. One form belongs to the language itself: in a
body of entries, `body json { title; author; }`, the block holds entries as `query` does, and
`json` is the path of a function that is called with them.

Every mistake in a description is a compile error with its own message, and the macro reports
all of its messages together, in one build. A mistake inside your Rust code (a type that is not
`Deserialize`, a reader that does not fit) is reported by `rustc`: it appears once the
description is correct, and one mistake may produce several errors.

The reference of the whole language, with an example for each statement, is the rustdoc page
of `concord::api!`, on docs.rs once Concord is published; until then, read [its source](https://github.com/gqsnt/concord_api/blob/main/concord-macros/src/reference.md). The guide explains it page by page: [describe an API](https://github.com/gqsnt/concord_api/blob/main/docs/guide/describing-an-api.md).

## Who depends on it

Nobody depends on it directly: depend on `concord-api` and write `concord::api! { … }`. The
code that the macro generates refers to `::concord`, and `concord-api` pins this crate and
`concord-core` to the same exact release, so the macro and the engine it generates code for
always match.

Minimum Rust version: 1.96.
