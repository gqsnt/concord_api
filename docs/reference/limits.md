# Limits reference

Every cumulative lifecycle surface is finite. Runtime setters replace these
defaults on the generated client builder; positive counts/byte limits use
`NonZeroU32` or `NonZeroUsize`.

The table is generated from normative core constants and checked by
`cargo run --locked -p concord-xtask -- docs`. `DEFAULT_*` entries are
selected runtime defaults; `MAX_*` entries are non-adjustable structural
ceilings selected for finite construction.

<!-- BEGIN GENERATED LIMITS -->
| Bound | Value | Unit |
|---|---:|---|
| Request body | 16,777,216 | bytes |
| Buffered successful response | 16,777,216 | bytes |
| Streaming response | 16,777,216 | bytes |
| Captured terminal status body | 65,536 | bytes |
| Credential-provider request body | 1,048,576 | bytes |
| Credential-provider response body | 1,048,576 | bytes |
| Pagination pages | 1,000 | pages |
| Pagination accumulated items | 100,000 | items |
| Retained pagination identities | 4,194,304 | bytes |
| Authentication-failure cooldown cap | 60 | seconds |
| Server-directed rate-limit cooldown cap | 60 | seconds |
| Default limiter-prepared rate-limit key | 1,024 | bytes |
| One formatted public value | 65,536 | bytes |
| Runtime endpoint top-level structure | 256 | components |
| Runtime endpoint aggregate public shape | 65,536 | bytes |
| Runtime endpoint authentication placements | 64 | placements |
| Rate-limit rule ID | 256 | bytes |
| Rate-limit key | 32 | parts |
| Rate-limit key-part name | 128 | bytes |
| Rate-limit hard aggregate key | 65,536 | bytes |
| Rate-limit rule | 16 | windows |
| Rate-limit plan | 64 | rules |
<!-- END GENERATED LIMITS -->

Dynamic host, path, query, and header values cross a user-code trust boundary
when Concord invokes their `Display` implementation. User formatting may do
arbitrary CPU work, block, panic, or ignore a formatting error. Concord's
capped capture primitive stops retaining UTF-8 output at
`concord::endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES` and rejects excess
rather than truncating it. The closure ledger tracks migration of every
request-shaping consumer to that primitive; finite formatted capture is not a
global completion claim until C2 and C4 are complete.

The inherited client operation timeout is disabled by default. Generated
endpoint timeout declarations remain effective independently; runtime-defined
endpoints select inherit, explicit duration, or disabled.

Terminal status-body capture is enabled at its finite default. The builder can
disable capture entirely; that does not create an unbounded mode.

Record decoders declare their own positive finite staging bound. Body
factories, providers, pagination controllers, limiters, observers, and hooks
must also bound their internal work and state; the runtime cannot infer
arbitrary extension storage.

Authentication challenge recovery permits at most one additional attempt.
Pagination is sequential and constrained simultaneously by its request
`PageLimit`, runtime page ceiling, item ceiling, retained identity ceiling,
and cycle detector. The first excess unit is withheld.

`EndpointId` additionally enforces its documented byte/segment grammar; use
`concord::endpoint::MAX_ENDPOINT_ID_BYTES` as the public source for its total
bound.
