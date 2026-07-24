# Limits reference

Every cumulative lifecycle surface is finite. Runtime setters replace these
defaults on the generated client builder; positive counts/byte limits use
`NonZeroU32` or `NonZeroUsize`.

The table is generated from the normative public core and extras constants and
checked by `cargo run --locked -p concord-xtask -- docs`. `DEFAULT_*` entries
are selected runtime defaults; `MAX_*` entries are non-adjustable structural
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
| Endpoint contract identity | 128 | bytes |
| Runtime endpoint top-level structure | 256 | components |
| Runtime endpoint aggregate public shape | 65,536 | bytes |
| Runtime endpoint authentication placements | 64 | placements |
| Rate-limit rule ID | 256 | bytes |
| Rate-limit key | 32 | parts |
| Rate-limit key-part name | 128 | bytes |
| Rate-limit hard aggregate key | 65,536 | bytes |
| Rate-limit rule | 16 | windows |
| Rate-limit plan | 64 | rules |
| OAuth2 token URL text | 8,192 | bytes |
| OAuth2 client identifier | 8,192 | bytes |
| OAuth2 client secret | 8,192 | bytes |
| OAuth2 scope | 65,536 | bytes |
| OAuth2 Basic Authorization value | 32,768 | bytes |
<!-- END GENERATED LIMITS -->

Dynamic host, path, query, and header values cross a user-code trust boundary
when Concord invokes their `Display` implementation. User formatting may do
arbitrary CPU work, block, panic, or ignore a formatting error. Concord's
capped capture primitive stops retaining UTF-8 output at
`concord::endpoint::MAX_FORMATTED_PUBLIC_VALUE_BYTES` and rejects excess
rather than truncating it. Runtime endpoints and generated/public-head host,
path, query, header, and pagination-sensitive shaping all use that primitive.

The optional OAuth2 client-credentials provider owns exact public ceilings for
token URL text, client identifiers, client secrets, scope text, and the final
Basic Authorization header under `concord::auth::MAX_OAUTH2_*`. It validates
borrowed configuration text before copying. Percent-encoded and Base64 output
lengths are checked before allocation, and the final form body is also checked
against the active `ProviderHttp::request_body_limit()` selected by the client
runtime.

The runtime-endpoint timeout default is disabled. Generated endpoint timeout
declarations remain effective independently; runtime-defined endpoints select
inherit, explicit duration, or disabled.

Public rate-limit construction validates IDs, key parts, aggregate keys,
windows, and plans incrementally against the hard `MAX_RATE_LIMIT_*`
ceilings. Duplicate key-part names and rule IDs are rejected without
consuming an iterator tail. A limiter may select a lower preparation-key
limit, including zero for global-only keys, but client construction rejects a
selection above `MAX_RATE_LIMIT_KEY_BYTES`.

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
