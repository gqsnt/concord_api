# concord-extras

The standard formats of [Concord](https://github.com/gqsnt/concord_api): `Json` (feature `json`),
`Text`, `Binary`, `Form` (feature `form`) and the adapter `IgnoreContentType<F>`. Each is an
ordinary implementation of the public `Encode`/`Decode` traits of `concord-core`; a format of your
own has the same standing.

Feature `multipart` adds `multipart::Multipart`, a request format for `multipart/form-data` and
`multipart/related` bodies (`multipart::MultipartBody`): the parts are held in memory, the boundary
is chosen once they are known so that it occurs in none of them (deterministic candidates,
checked rather than random), and it travels with the bytes as the body's runtime media type. It
needs no dependency.

Applications normally get these formats through the `concord` crate's features. This crate does
not depend on the macro or on `concord`.
