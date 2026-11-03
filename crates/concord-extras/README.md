# concord-extras

The standard formats of [Concord](https://github.com/gqsnt/concord_api): `Json` (feature `json`),
`Text`, `Binary` and `Form` (feature `form`). Each is an ordinary implementation of the public
`Encode`/`Decode` traits of `concord-core`; a format of your own has the same standing.

Applications normally get these formats through the `concord` crate's features. This crate does
not depend on the macro or on `concord`.
