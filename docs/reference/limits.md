# Limits reference

Every cumulative lifecycle surface is finite. Runtime setters replace these
defaults on the generated client builder; positive counts/byte limits use
`NonZeroU32` or `NonZeroUsize`.

The table is generated from the single normative core defaults source and
checked by `cargo run -p concord-xtask -- docs`.

<!-- BEGIN GENERATED LIMITS -->
| Runtime bound | Default | Unit |
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
| Resolved rate-limit key | 1,024 | bytes |
<!-- END GENERATED LIMITS -->

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
