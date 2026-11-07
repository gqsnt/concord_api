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
