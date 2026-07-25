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
| Final managed request head | 256 | components |
| Logical or credential-sanitized request URL | 131,072 | bytes |
| Public request headers before authentication | 256 | headers |
| Aggregate public request headers | 65,536 | bytes |
| Proxy URL text | 8,192 | bytes |
| Custom root certificates | 64 | certificates |
| One custom root certificate PEM | 1,048,576 | bytes |
| Aggregate custom root certificate PEMs | 16,777,216 | bytes |
| Application identity PEM | 4,194,304 | bytes |
| Endpoint contract identity | 128 | bytes |
| Authentication placement name | 256 | bytes |
| Generic protected string | 1,048,576 | bytes |
| Raw credential secret | 65,536 | bytes |
| Basic authentication username | 8,192 | bytes |
| Basic authentication password | 32,768 | bytes |
| Native authentication header value | 65,536 | bytes |
| Encoded authentication query value | 65,536 | bytes |
| Multipart recipe | 128 | parts |
| Multipart part name | 256 | bytes |
| Multipart filename | 1,024 | bytes |
| Multipart part content type | 256 | bytes |
| Multipart aggregate metadata | 65,536 | bytes |
| Multipart inline text | 16,777,216 | bytes |
| Runtime endpoint top-level structure | 256 | components |
| Runtime endpoint aggregate public shape | 65,536 | bytes |
| Authentication plan | 64 | placements |
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

The final core `HeadBuilder` is authoritative for both generated and
runtime-defined requests. It transactionally enforces component count,
logical and credential-sanitized URL bytes, public-header count, and aggregate
public-header bytes. Macro checks reject statically knowable excess earlier,
but cannot replace this shared runtime boundary.

With the `json` feature, `Json<T>` serializes directly into a writer capped by
the active request-body limit. It never creates an unrestricted serialized
`Vec` and then checks it, and it retains no byte from the first excess write.
The final core body-length check remains as a defense shared by every codec.

`SecretString` validates its generic retained-storage ceiling before copying,
and clones share the same immutable allocation. Generated client construction
validates built-in static credentials against their tighter API-key, bearer,
and Basic limits before creating the immutable snapshot or credential slots;
generated replacement validates before slot mutation. After limiter authority
and credential finalization, core defensively validates borrowed secret
material from every source, computes Bearer and Basic output with checked
arithmetic, and bounds the final private header or percent-encoded query value
before mutating the native request. These hard limits are exposed under
`concord::auth`.

Multipart constructors validate borrowed names, filenames, content types, and
inline text before copying them. Recipes retain at most 128 parts and track
aggregate metadata incrementally; factory output is revalidated before private
native conversion. The active request-body limit still bounds serialized
payload and framing and may be lower than these structural ceilings. Multipart
limits are exposed under `concord::body`.

The optional OAuth2 client-credentials provider owns exact public ceilings for
token URL text, client identifiers, client secrets, scope text, and the final
Basic Authorization header under `concord::auth::MAX_OAUTH2_*`. It validates
borrowed configuration text before copying. Percent-encoded and Base64 output
lengths are checked before allocation, and the final form body is also checked
against the active `ProviderHttp::request_body_limit()` selected by the client
runtime.

Managed HTTP startup validates proxy URL text and application TLS material
against the structural `concord::http::MAX_*` ceilings before parsing or
copying it. Root-certificate count, per-certificate bytes, aggregate
certificate bytes, and application identity bytes are all finite. Native URL
and TLS parsers may reject structurally bounded inputs more strictly; startup
errors retain only a safe category and never retain supplied proxy or PEM
content. Custom roots and proxy configuration are inherited by both
application and provider clients, while the application identity remains
application-only.

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
