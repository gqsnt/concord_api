# Wave 1 → Wave 2 candidate changes (draft, before SDK23/SDK24 close)

Ordered by evidence. Each needs a compiled before/after with unlike witnesses and a simple control
before acceptance; dispositions are final only in `SYNTHESIS.md`.

| # | Candidate | Witnesses (W1 + earlier) | Placement question | Simple-control consequence |
|---|---|---|---|---|
| A | Declared finite-body limit honoured by every finite decode path (generated `send()`, `Received::decode`, `prepare()?.send()`), so a policy is not needed only to bound bodies and the native path cannot bypass it. | SDK21, SDK22, SDK25 (F4), SDK23 (finite vs stream limits); SDK20: Dropbox, Slack, Linear, Drive, Airtable, Snowflake, World Bank, Notion, Graph, registry (`decode_limited`) | MACRO (statement on client/scope/endpoint) + CORE_CHANGE (`Decoder` carries the limit) vs EXTERNAL policy combinator | Nager-style SDK without a policy gets bounded decoding from one line |
| B | Generated docs: link the send policy type and say where its output is defined; list an endpoint's included parameter groups; show the format's `Accept`. | SDK22 consumer, SDK21 maintainer, SDK22 maintainer, SDK25 consumer | MACRO (docs only) | none |
| C | Strict single-valued numeric header reading (absent / repeated / unparsable / value) on `ResponseHead`. | SDK22 maintainer (X-RateLimit), GitHub/GitLab rate & pagination headers (`+5` accepted, absent = unparsable), Dropbox/Slack (now `concord-retry::http`) | CORE (borrowed head view) vs codecs/pagination helper | none (opt-in method) |
| D | Public strict media/charset check in `concord-codecs` for custom formats. | SDK25 F3 (XML, ZIP, problem documents re-implemented with `mime`); CUSTOM_FORMATS guide | EXTERNAL (codecs) | none |
| E | Offset-envelope helper (count/offset/limit arithmetic, non-progress) in `concord-pagination`. | SDK25 F6, SDK23 (offset), SDK21 (page/offset), PokéAPI | EXTERNAL | none |
| F | One operation, several representations (XML/JSON/CSV/GeoJSON by a query parameter + Accept + format). | SDK25 F5, SDK23 (1), SDK20 Open-Meteo, ClickHouse | MACRO experiment (endpoint variants) vs retain separate endpoints + groups | must not complicate single-format endpoints |
| G | Rate admission (route buckets, global 429, cooldown) | SDK24 (pending) | EXTERNAL `concord-ratelimit` pilot | none |
| H | Docs: version prefix in a `path` statement rather than `base` (so `with_base_url` keeps it); query-credential transport recipe (no redirects/referrer/native retries); `concord::Error` Display/Debug are safe, accessors are explicit unredacted escapes. | SDK22 consumer, SDK25 F7, SDK22/SDK21 authors | DOCS | none |
| I | XML codec in `concord-codecs` | SDK25 F1 (+ S3 `ListXml`) | EXTERNAL, wait for a third unlike witness (arXiv, Europe PMC in later waves) | — |
| J | Bounded ZIP member reader | SDK25 F2 only | OMIT for now (one provider) | — |
