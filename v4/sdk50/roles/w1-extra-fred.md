## Slice-specific notes

FRED serves observations as XML (default), JSON, and a zipped CSV export. Concord's codecs have no
XML or ZIP support: choose the simplest correct route (an XML parser dependency, a custom
`Decode` format, a native escape), keep it bounded (archive and decompressed-size limits), and
record the cost in `FRICTION.md` — this is evidence for whether a shared XML codec is justified.
The API key is a query parameter and must never appear in `Debug`/`Display` or error text.
