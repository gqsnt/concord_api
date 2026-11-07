# Authored fixture contract

All payloads are synthetic. Exact documentation URLs, read date 2026-11-07 and derivations
are in [PROVENANCE](../PROVENANCE.md). No payload was captured from the live service.
This file specifies expectations independently of SDK implementation details.

The connected topic is `malaria`, with a requested pageSize of 2 and lite results.
JSON and XML represent the same records. The first request uses cursorMark `*`; the
first reply returns `p+1/==?&`, which is opaque text. The next request sends that exact
text as one encoded query value. The final reply contains MED/20485429 and has no cursor.
The expected ordered identities are MED/20485428, PMC/PMC7561308, MED/20485429.

Selecting MED/20485428 and requesting `/MED/20485428/citations` with format=json,
page=1, pageSize=1 yields MED/30000001. A core search for that exact source/identifier
returns one synthetic citation whose abstract is
`Authored fixture abstract; no scientific claim.`

A one-page traversal cap preserves the first two records and the unrequested next cursor.
Resuming the same query/page-size/format requests that next page directly. A returned
cursor already used in this traversal is a failure; the offending page contributes no
records. An empty page with the fresh `empty+next==` cursor continues, so the sequence
`*` → `p+1/==?&` → `empty+next==` → `p+1/==?&` is a three-request failure.
An empty result without a cursor completes immediately.

The media label must agree with the selected decoder. A server ignoring format=json and
returning its XML default is refused before JSON parsing. If it lies with a JSON media
label, XML bytes are a decode error. A malformed or truncated body is not an empty result.
Oversized success/error bodies, failed chunks and stalled bodies are synthetic transport
faults constructed in protocol.rs; they do not require separate large fixture files.

## Reference list (maintenance addendum, 2026-11-07)

`references-{1,2,3}.{json,xml}` are authored synthetic pages of the reference list of
MED/20485428, requested at pageSize 2 with page = 1, 2, 3 on
`/MED/20485428/references`. Every page reports hitCount 5. JSON and XML carry the same
records; the envelope (`referenceList/reference`) mirrors the documented citations envelope
(`citationList/citation`), see [MAINTENANCE](../MAINTENANCE.md) for what was and was not
observed. The ordered list is:

| Page | Records |
| --- | --- |
| 1 | MED/31000001 (`pubYear` is the JSON number 2001); an unmatched reference without id or source (`pubYear` "2002") |
| 2 | PMC/PMC9000003; MED/31000004 (JSON number 2004) |
| 3 | MED/31000005 |

Page 3 reaches the hit count, so a traversal ends there without a fourth request. A cap of
two pages keeps four records and resumes at page 3. A later page that fails keeps the
records of the pages before it and resumes at the failed page. A short page before the hit
count, an empty page before it, a changed hit count, or a page beyond it is a failure, not
a partial success. A zero hit count with or without an empty `referenceList` is a complete,
empty list. The contradiction and malformed bodies are inline literals in
`tests/references.rs`; `tests/references_use.rs` builds its own bodies.
