# concord-macros

The procedural macro of [Concord](https://github.com/gqsnt/concord_api): the `api!` declaration
compiler. It turns a client → scopes → endpoints declaration into a typed client whose generated
code calls `concord-core`.

Use it through the `concord` crate's `concord::api!`, which passes the runtime crate's path to this
macro; depending on `concord-macros` directly is not supported. The macro knows no format: a
format statement names a Rust type path, resolved where it is written.
