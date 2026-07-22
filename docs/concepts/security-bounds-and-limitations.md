# Security, bounded I/O, extension boundaries, and limitations

> Audience: Users and extension authors

Concord is intentionally narrower than a general HTTP toolkit. This page explains the safety model, the finite-resource rules, and the supported alternatives when a capability is outside the product boundary.

## Bounded I/O

Concord treats finite resource behavior as part of the API contract.

### Independent bounds

- application request body;
- buffered successful response;
- streaming successful response;
- terminal non-success status body;
- provider request and response;
- pagination pages and collected items;
- canonical pagination identity storage;
- record decoder staging;
- advanced rate-limit key bytes;
- governor quota and cooldown entries.

These bounds are independent because they protect different lifecycle stages.

### Known length and first excess

When a length is known before materialization, Concord can reject excess before hooks or body polling. When length emerges incrementally, the first frame/chunk/record append that would exceed the bound is withheld. The caller does not receive a partial successful semantic value.

### Complete-only buffered values

Buffered codecs receive complete bounded bytes or an error. Status-body capture is similarly complete-only. A decoder never receives a retained prefix of an oversized body.

### Streams

Stream limits count application bytes after decompression. The stream yields safe errors instead of an offending excess chunk.

### Record staging

Record staging protects the undecoded fragment, independent of the total stream limit. This prevents one malformed or very large record from accumulating indefinitely in an otherwise bounded response.

### Disabling is not unbounded

Disabling status-body capture means discard. There is no ordinary setter that turns a positive finite body or pagination limit into infinity.

### Application extensions

Concord bounds its own state. Applications must still bound custom provider caches, limiter queues, hook metrics, decoder state beyond the core staging buffer, and any external I/O they initiate.

## Extension boundaries

Concord uses narrow interfaces so applications can change semantics without replacing execution.

### Buffered codecs

Receive or produce complete bytes. They do not own status handling, native response access, or body streaming.

### Request body specifications

Describe metadata and produce a managed body. Core validates consistency, derives replayability, applies limits, and controls when preparation occurs.

### Response body specifications

Select one sealed acquisition mode. Core supplies complete bytes, a bounded byte stream, or unit. Applications cannot define an unbounded fourth mode.

### Record decoders

Frame semantic records from a bounded staging buffer over a bounded byte stream. They do not poll native HTTP directly.

### Credential providers

Own credential protocols and material. Provider HTTP is isolated from application execution.

### Pagination controllers

Own deterministic state transitions. They affect requests only through explicit bound endpoint/scope fields.

### Rate limiters

Own authority for a semantic plan. They receive no body, credential, authentication material, native URL, or native request.

### Hooks

Observe sanitized lifecycle events. They cannot mutate the native request or change the fixed result.

### Prepared endpoints

Provide runtime route composition but still enter the same generated-client capability and core runtime.

### Design test

A proposed extension is suspicious when it needs:

- `reqwest::Response`;
- an application-defined executor;
- a public execution plan;
- `Any` downcasting;
- unbounded bytes;
- a second completion path;
- direct mutation of shared client state.

Prefer a semantic type or trait that core can call at the correct lifecycle boundary.

## Security model

Concord's security model focuses on ownership boundaries, safe defaults, finite resources, and reducing accidental disclosure.

### Logical versus native request

The logical URL and public headers are built without authentication. Hooks, canonical identity, and safe debug use that logical state. Authentication is materialized only into the private native request immediately before send.

### Secret types and slots

Secrets enter generated client construction through dedicated secret inputs. Credential material is stored in typed private slots with versioning, single-flight coordination, expiration, clear/replace operations, and stale-completion protection.

### Redaction

Built-in `Display`, `Debug`, recursive sources, hooks, completion, and Rustdoc avoid secret, body, record, continuation, advanced-key, provider, and native-URL values. Sanitized public headers can still be operationally sensitive and should be protected.

### Intentional access

Security does not mean values are inaccessible to the application. Explicit accessors and selected extensions intentionally receive values:

- `SecretString::expose_secret`;
- status-body byte accessors;
- raw successful response bytes;
- codec and record inputs;
- custom limiter explicit keys;
- provider HTTP responses.

The caller owns those uses.

### Transport

Credential materialization is checked against the managed HTTPS capability and final destination. Native requests, responses, clients, and authenticated URLs remain private.

### Cancellation and locks

Core does not hold its state locks across `await` and does not run application callbacks under core locks. Cancellation drops owned futures and bodies. User/provider destructors run outside credential-state locks.

### Limits

Finite limits reduce memory/resource attacks but are not a substitute for server trust, input validation, application authorization, or external rate enforcement.

### Dangerous features

Raw success and custom native retry deliberately widen what application code can observe. They are opt-in, binary-wide Cargo features and should be reviewed as separate threat-model decisions.

## What Concord is not

Concord is intentionally narrower than a general HTTP framework.

### Not a transport abstraction

Reqwest is privately managed. Applications cannot install another executor, transport, middleware stack, native request, or native response. Use another HTTP library directly when transport control is the primary requirement.

### Not a runtime schema interpreter

APIs are generated at compile time. Concord does not load OpenAPI or endpoint schemas dynamically at runtime.

### Not a distributed quota system

The built-in governor is process-local pacing. Use a custom limiter for shared or authoritative quota decisions.

### Not a protocol framework

`ByteStream` and `RecordDecoder` support bounded byte/record protocols, but Concord does not implement WebSockets, SSE reconnection, bidirectional sessions, multipart response parsing, or arbitrary protocol state machines.

### Not an unbounded downloader

Ordinary body and stream acquisition is finite. Applications needing very large transfers must choose explicit finite limits and consume streams incrementally.

### Not a page-stream API

Pagination returns an all-or-error vector. It does not prefetch, parallelize, stream items, expose partial success, or paginate prepared endpoints.

### Not a secret-management system

Concord stores and coordinates credential material for request execution. It does not provide encryption at rest, OS keychain integration, zeroization guarantees, authorization policy, or secret rotation infrastructure beyond source-specific handles/providers.


## Unsupported behavior and alternatives

This page is the explicit product boundary.

| Not supported | Why | Nearest supported alternative |
|---|---|---|
| Native Reqwest request/response access | would bypass status, limits, redaction, and ownership | semantic hooks/specifications; dangerous bounded raw success for successful bytes |
| Custom transport or executor | would create a parallel lifecycle | use another HTTP library directly |
| Runtime API schema loading | Concord is compile-time generated | generate Rust source before compilation |
| Application-defined response acquisition mode | core must bound native body ownership | choose `Buffered`, `Streaming`, or `Discard` |
| Unbounded request/response/stream/status/record mode | finite behavior is a core guarantee | choose an explicit larger finite bound |
| Prepared pagination | no approved controller/binding contract | generate a static paginated endpoint or paginate in application code |
| Streaming pagination/items | would introduce partial-success/completion semantics | sequential `.paginate()` returning a vector |
| Page prefetch or parallel pages | conflicts with sequential controller authority and effects | application-controlled independent requests |
| Partial pagination success | current result is all-or-error | use direct page requests when partial retention is needed |
| `.collect()` pagination terminal | `.paginate(limit).await` is already the terminal | await the paginated request |
| WebSocket support | bidirectional transport is outside managed HTTP response acquisition | use a WebSocket library separately |
| SSE reconnection framework | reconnection/state semantics are protocol-specific | consume `ByteStream` or implement a bounded decoder; manage reconnection in application code |
| Multipart response parsing | not part of core record framing | custom streaming response specification or specialized parser outside core |
| Distributed/strict built-in quota | governor is bounded process-local pacing | custom `RateLimiter` backed by shared authority |
| Generated per-user/per-item/per-region-value keys | normal DSL uses lexical ownership and global keys | custom limiter or prepared explicit `RateLimitKey` |
| Generated cross-branch quota groups | no coherent lexical owner | custom limiter or prepared explicit rules |
| Endpoint opt-out from ancestor rate limit | ancestors define subtree policy | restructure the API tree or use custom limiter policy |
| Arbitrary DSL Rust expressions | would execute/format uncontrolled application logic | prepare values in Rust, pass typed parameters, or implement an extension |
| Secrets in host/path/query/header expressions | prevents accidental disclosure | use credentials and `auth` placements |
| Automatic rollback of hook/provider/remote effects | external effects are not transactional | design idempotent extensions and compensation explicitly |
| Automatic timeout for completion hooks | completion observes fixed result outside deadline | bound hook work internally |

When a missing capability is central rather than incidental, prefer a dedicated library instead of forcing it through Concord's advanced boundary.
