# concord-core

The runtime of [Concord](https://github.com/gqsnt/concord_api): request composition, native
headers, snapshots, the open `Encode`/`Decode` contracts and the error type. It depends on
reqwest, `bytes`, `url`, `percent-encoding` and `http-body` (already reqwest's), and on nothing
format-specific.

Applications normally depend on the `concord` crate, which re-exports this crate's public API
together with the `api!` macro and the standard formats. Depend on `concord-core` directly to
implement a format (`Encode`/`Decode`) without the macro or the standard formats.

The `__private` module is support code for `api!` expansions, not a public API.
