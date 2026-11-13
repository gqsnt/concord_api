# concord-macros

The macro `api!` of Concord (the crate `concord`). It reads a nested description
of an HTTP API and writes a typed client: a client type, one method for each endpoint, one
struct for each call, and one error type.

The macro knows the tree, the statements and the inputs. It knows no format, no scheme and
no pager: what stands after `read`, `paginate` or `body` is a Rust expression, and the
compiler checks it against a trait of the core. One form is the language's own: in a body of
entries, `body json { title; author; }`, the block holds entries as `query` does, and `json`
is the path of a function that is called with them.

Every mistake in a description is a compile error with its own message, and the macro's own
messages are shown together, in one build. A mistake inside your Rust (a type that is not
`Deserialize`, a reader that does not fit) is `rustc`'s: it comes once the description is
right, and one mistake may give several errors.

The whole language, with an example for each statement, is the documentation of
`concord::api!`.

## Who depends on it

Nobody, directly. Depend on `concord` and write `concord::api! { … }`. The code that the
macro writes names `::concord`, and `concord` pins this crate and `concord-core` to the same
exact release, so that the macro and the engine that it writes for always match.

Minimum Rust version: 1.96.
