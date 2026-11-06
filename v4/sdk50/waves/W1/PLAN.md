# Wave 1 (discovery) — SDK21–25

Frozen library baseline: the campaign head after P1 (recorded in `STATUS.md` of this wave when the
wave starts). During the wave no shared crate changes; authors, consumers and maintainers own only
their SDK directory. Shared findings go to the between-wave synthesis (`SYNTHESIS.md`).

| SDK | Provider (size) | Author | Fresh consumer | Maintainer | Hypotheses to test (cohort) |
|---|---|---|---|---|---|
| SDK21 | Open Library (small) | Sol 6.1 | Terra | Astra | plain JSON, page/limit; simple control for the wave |
| SDK22 | NASA NeoWs (small) | Terra | Astra | Opus | sensitive query API key; browse pagination; quota errors without leaking the key |
| SDK23 | USGS Earthquake Catalog (medium) | Astra | Opus | Sol 6.1 | GeoJSON + CSV rows incrementally; offset; runtime format choice (F2-5) |
| SDK24 | Discord REST (medium) | Opus | Sol 6.1 | Terra | bot token; before/after cursors; route buckets + global 429; ambiguous write (rate limiting evidence for X-5) |
| SDK25 | FRED (medium) | Sol 6.1 | Terra | Astra | XML + JSON + zipped CSV; query API key; offset (XML codec evidence) |

Role rotation follows the handoff's ROUTING table (author at Sol for SDK21, advancing one family
per SDK; consumer = author + 1, maintainer = author + 2). Requested effort xhigh for all.
Evidence labels: authors are example-assisted when they read other SDKs (recorded); consumers start
docs-only; maintainers attempt their ticket before reading friction notes.

Questions this wave must answer about Concord (beyond delivering five SDKs):
1. Do new authors find and use parameter groups, `Received::document`, `decode_with_meta`,
   `Sends<O>` and the Link parser without coaching, or re-implement them?
2. Is the F2 policy skeleton now short, or does a new repeated mechanic appear?
3. Which F3 experiments get a second witness: getters (F3-3), dynamic query keys / policy-only
   inputs (F3-4), runtime format choice (F2-5)?
4. What does a rate-limited provider (Discord) write by hand that `concord-ratelimit` must absorb?
5. Simple control: does SDK21 stay a short declaration plus models?
