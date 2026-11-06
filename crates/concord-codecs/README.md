# concord-codecs

The standard formats of [Concord](https://github.com/gqsnt/concord_api): `Json` (feature `json`),
`Text`, `Binary`, `Form` (feature `form`) and the adapter `IgnoreContentType<F>`. Each is an
ordinary implementation of the public `Encode`/`Decode` traits of `concord-core`; a format of your
own has the same standing.

Features (none is on by default):

- `json`: `Json`, via serde and serde_json.
- `form`: `Form` (`application/x-www-form-urlencoded`), via serde and serde_urlencoded.
- `stream`: the incremental-body infrastructure of the streaming formats (`stream`): response
  opening with unread refusals, byte sources, safe read errors, `RecordStream`, lazy uploads.
- `csv` (implies `stream`): `Csv<HasHeader, Delimiter>`, finite and incremental.
- `jsonl` (implies `stream`): JSON Lines / NDJSON, finite and incremental, and consecutive JSON
  values.
- `sse` (implies `stream`): server-sent events (`text/event-stream`) read incrementally, with
  the `Sse` format for `-> stream sse::Event`; no serializer dependency (`data` is not decoded).
- `multipart`: `multipart::Multipart`, `multipart/form-data` and `multipart/related` request
  bodies with a boundary chosen per body; no dependency.

Feature `multipart` adds `multipart::Multipart`, a request format for `multipart/form-data` and
`multipart/related` bodies (`multipart::MultipartBody`): the parts are held in memory, the boundary
is chosen once they are known so that it occurs in none of them (deterministic candidates,
checked rather than random), and it travels with the bytes as the body's runtime media type. It
needs no dependency.

Applications normally get these formats through the `concord` crate's features. This crate does
not depend on the macro or on `concord`.
