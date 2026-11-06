# concord-extras

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

Applications normally get these formats through the `concord` crate's features. This crate does
not depend on the macro or on `concord`.
