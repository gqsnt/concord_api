# CONCORD_SECURITY_AND_LIMITS_SPEC_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate
**Authority:** Security, secrecy, redaction, fail-closed behavior, and exact resource bounds  
**Requirement prefixes:** `SEC`, `BOUND`  
**Depends on:** Specification Index, Product Contract, Public API Specification  
**Supersedes:** None

## 1. Threat model

Concord protects against accidental exposure and accidental unbounded behavior through its ordinary APIs.

The protected surfaces include:

- error `Display` and `Debug`;
- Concord-controlled error source chains;
- hooks;
- built-in debug output;
- rate-limit contexts and built-in errors;
- pagination diagnostics;
- generated Rustdoc;
- wrapper `Debug` implementations;
- safe HTTP configuration errors.

Concord assumes application code running in the same process is trusted to access the values explicitly supplied to its extension points.

Concord does not protect against:

- malicious application code;
- a debugger or process memory dump;
- operating-system compromise;
- application code explicitly calling secret exposure methods;
- application-owned logging, metrics, panic messages, storage, or network side effects;
- unsafe or dangerous feature misuse.

**SEC-THREAT-001 — Accidental exposure boundary.** Concord MUST prevent protected values from appearing through ordinary Concord-owned diagnostics and callbacks.

**SEC-THREAT-002 — Trusted extension boundary.** Concord MAY supply sensitive inputs to documented extensions that require them, and the extension author is responsible for side effects outside Concord's control.

## 2. Data classification

| Class | Examples | Ordinary exposure |
|---|---|---|
| DSL secret | API key input, password, OAuth client secret | Never |
| Credential material | bearer token, API key, Basic username/password, OAuth access token | Never |
| Public route data | host label, path segment, ordinary query value | Application-visible; sanitized in callbacks when name-sensitive |
| Public request header | tenant header, idempotency key | Safe values visible in hooks/debug; sensitive names redacted |
| Request body | JSON, text, bytes, stream, multipart fields and filenames | Never through ordinary diagnostics |
| Successful response data | decoded value, successful headers, bytes, stream chunks | Intentionally available to application |
| Terminal non-success response body | final `3xx`/`4xx`/`5xx` body | Explicit finite complete-only access through `HttpResponseFailure`; never automatically formatted or observed |
| Provider request/response data | token request and token response | Provider only |
| Pagination continuation | cursor, offset, controller state | Controller/runtime only; absent from safe errors |
| Rate-limit key | tenant/account/host dimensions | Selected limiter only; absent from built-in errors |
| Logical URL | public URL before auth query | Successful metadata; sanitized callbacks |
| Native URL | URL after auth query | Reqwest and dangerous custom retry only |

**SEC-DATA-001 — Declaration matters.** A value declared as an ordinary public value is not automatically treated as a secret because its runtime contents are sensitive.

**SEC-DATA-002 — Secret system requirement.** Credentials and credential-equivalent values SHOULD be represented through the credential system rather than public route, query, or header values.

## 3. Secret values

### 3.1 Secret formatting

**SEC-SECRET-003 — Redacted secret formatting.** `SecretString`, built-in credential materials, and custom `SecretCredential` implementations MUST render a redacted placeholder through ordinary `Display` and `Debug`.

The canonical placeholder is:

```text
<redacted>
```

### 3.2 Secret use

DSL secrets may be used only for built-in credential construction.

They cannot participate in:

- host;
- path;
- public query;
- public headers;
- formatting;
- timeout;
- rate-limit dimensions;
- pagination configuration.

### 3.3 Secret memory limitations

Concord does not guarantee:

- memory zeroization;
- encrypted memory;
- non-swappable memory;
- removal from allocator history;
- protection after explicit exposure.

**SEC-SECRET-001 — Safe formatting.** Concord-provided secret and credential types MUST not reveal values through ordinary formatting.

**SEC-SECRET-002 — No DSL public flow.** The DSL MUST make it impossible to reference a secret through ordinary request-shaping expressions.

## 4. Logical and native URLs

### 4.1 Logical URL

The logical URL contains:

- public scheme;
- public host;
- public path;
- public query pairs.

It does not contain Concord-managed query authentication.

Successful response `url()` accessors expose the logical URL.

### 4.2 Safe URL

Hooks, debug output, rate limiters, and ordinary errors use a safe rendered URL derived from the logical URL.

Sensitive query values are replaced by `<redacted>`.

### 4.3 Native URL

The native URL may contain query authentication.

It is private to:

- Reqwest;
- dangerous custom retry callbacks.

**SEC-ERR-001 — Native error reduction.** A raw native transport error MUST NOT be retained in ordinary public errors.

**SEC-URL-001 — Query-auth exclusion.** Concord-managed query authentication MUST be absent from every logical and safe URL surface.

**SEC-URL-002 — Native error stripping.** Native URLs MUST be stripped before a Reqwest error is retained or rendered through ordinary Concord APIs.

## 5. Sensitive-name classification

Sensitive names are matched ASCII case-insensitively after normalizing `_` and `-` as separators.

The mandatory sensitive categories include names containing or equal to:

```text
authorization
proxy-authorization
cookie
set-cookie
token
access-token
refresh-token
secret
password
passwd
credential
session
api-key
apikey
client-secret
```

Implementations may classify additional names as sensitive.

**SEC-HEAD-003 — Mandatory header classifications.** Custom sensitivity classifiers MUST NOT classify fewer than the mandatory sensitive-header categories.

**SEC-NAME-001 — Conservative classification.** The sensitive-name classifier MAY redact more names than the mandatory list but MUST NOT expose a mandatory sensitive category.

## 6. Header sanitization

Safe header views preserve:

- header name;
- presence;
- repeated-value count and iteration order.

Values are represented as:

- original UTF-8 text for non-sensitive textual values;
- `<redacted>` for sensitive names;
- `<non-utf8>` for non-sensitive non-text values.

A retained status error stores an owned sanitized header map, never the original raw header map.

**SEC-HEAD-001 — Sensitive value removal.** Every value of a sensitive header name MUST be replaced before safe callback or error access.

**SEC-HEAD-002 — No raw escape.** A sanitized header wrapper MUST NOT provide a method recovering hidden original sensitive values.

## 7. Query sanitization

**SEC-URL-003 — Safe URL redaction inputs.** The safe URL renderer MUST redact:

- every Concord-managed auth query key;
- every public query key classified as sensitive.

It preserves:

- query pair order;
- duplicate keys;
- non-sensitive public values.

Successful logical URL accessors intentionally retain ordinary public query values.

**SEC-QUERY-001 — Auth-key redaction.** An auth query value MUST be redacted even when its key is not otherwise classified as sensitive.

## 8. Request bodies

**SEC-BODY-002 — Request body diagnostic exclusion.** Request-specification inputs, prepared request
bodies, stream chunks, multipart values, encoded bytes, factory error text, and produced managed
body values MUST NOT appear in:

- `ApiError` display/debug;
- built-in source chains;
- hooks;
- debug output;
- rate-limit contexts;
- pagination errors;
- generated Rustdoc.

This includes:

- codec input/output;
- raw bytes;
- stream chunks;
- multipart text;
- multipart bytes;
- multipart filenames;
- body producer error text containing body fragments.

The selected request specification intentionally receives its documented input and owns its own
logging behavior. Concord forwards only structural `RequestBodyMetadata` to `before_send`; no hook,
limiter, debug surface, authentication surface, or native request diagnostic receives the input or
produced body.

Safe body errors may include numeric metadata:

- configured limit;
- exact declared length;
- observed byte count;
- body category.

**SEC-BODY-001 — No request body diagnostics.** Concord-owned ordinary formatting, recursive sources, callbacks, request debug, response debug, completion, pagination, limiter contexts, Rustdoc, and generated Rustdoc MUST contain no request-specification input, encoded bytes, stream chunks, multipart values, filenames, or managed outgoing body values.

**SEC-BODY-003 — Request-specification extension isolation.** A selected `RequestBodySpec` MAY receive only its documented input and `RequestBodyContext`; metadata MUST expose only structural facts, and no request specification MAY receive URL, headers, credentials, authentication, limiter, native request, Reqwest, or transport state.

## 9. Response bodies

### 9.1 Terminal non-success responses

A final non-success application response may retain a small finite body only when complete capture succeeds within the configured status-body limit and remaining operation deadline.

The body is intentionally available only through `ApiError::http_response()` and `HttpResponseFailure::body()`. It is never automatically rendered, logged, passed to hooks, passed to limiters, or included in source chains.

Known oversized bodies are not polled. Unknown-length bodies that exceed the limit expose no retained prefix. Body read failure or deadline expiry preserves the known HTTP status and records only a structural `StatusBodyState`.

A challenge response that will be followed by authentication recovery remains unread. A final challenge response may use the terminal status-body capture rules.

### 9.2 Successful buffered responses

Successful bytes are intentionally provided to:

- the selected buffered response specification or codec;
- bytes response callers;
- dangerous raw response callers.

### 9.3 Successful streams

Successful stream chunks are intentionally provided only through the selected streaming response
specification's `ByteStream` and its returned application value. The selected response extension
intentionally receives the documented successful `ResponseHead` and owns its own logging behavior.

### 9.4 Record streams

Record bytes, CSV headers and fields, JSON values, parser-authored text, and downstream decoder state
are intentionally available only to the selected record decoder as required by its public contract.
Concord record errors, sources, stream debugging, completion, and other automatic surfaces contain
only safe structural categories and approved safe messages.

### 9.5 Diagnostics

**SEC-RESP-003 — Response body diagnostic exclusion.** Buffered bytes, `ByteStream` chunks, record bytes, decoded values, response-specification output, and parser-authored unsafe text MUST NOT appear in:

- `ApiError` display or debug;
- Concord-controlled source chains;
- hooks;
- built-in debug output;
- limiter contexts;
- completion events;
- wrapper `Debug`.

**SEC-RESP-006 — Structural raw-response debugging.** `RawResponse::Debug` MUST report only structural successful metadata and body length rather than headers or body bytes, and response-specification errors MUST not reproduce application text or response data.

**SEC-RESP-001 — Explicit terminal-body boundary.** A terminal non-success body MUST be exposed only through the finite opaque `HttpResponseFailure` status-body API and MUST remain absent from every automatic diagnostic or callback surface.

**SEC-RESP-002 — Structural wrapper debug.** Successful and terminal status-response wrapper `Debug` implementations MUST NOT print decoded values or body bytes.

**SEC-RESP-007 — No truncated body exposure.** Concord MUST discard all collected terminal status-body bytes when complete capture cannot finish within the configured limit; buffered and streaming success acquisition MUST withhold the first excess chunk from response specifications and callers.

**SEC-RESP-008 — Terminal status preservation.** Status-body read failure, limit exhaustion, deadline expiry, or response-specification eligibility MUST NOT replace a known terminal HTTP status with a response-body category or invoke successful response validation.

**SEC-RESP-009 — Structural ApiResponse debugging.** `ApiResponse<T>::Debug` MUST be structural and MUST NOT format `T`, response headers, or response body data; `DecodedResponse<T>` MUST inherit exactly that implementation through its alias.

**SEC-STREAM-001 — ByteStream diagnostic exclusion.** `ByteStream::Debug` MUST NOT consume or inspect native body state, stream chunks, response metadata, or media data, and safe stream errors and recursive sources MUST reproduce none of those values.

**SEC-RECORD-001 — Record-data diagnostic exclusion.** Record bytes, CSV headers or fields, JSON values, downstream decoder state, and parser-authored text MUST remain absent from Concord formatting, recursive sources, hooks, debug, completion, limiter contexts, Rustdoc, and generated Rustdoc.

**SEC-RECORD-002 — Record-parser error sanitization.** CSV, NDJSON, and downstream record-parser failures MUST map to structural `RecordErrorKind` plus only explicitly application-approved safe messages; native or parser-authored unsafe text MUST NOT enter formatting or recursive sources.


## 10. Provider data

Provider HTTP data is available only to the custom or built-in provider executing the operation.

Application hooks, debug output, application limiter, pagination, and endpoint codecs receive no provider request or response data.

**SEC-PROV-002 — Provider error exclusions.** Built-in provider errors MUST exclude:

- token URL query;
- request body;
- response body;
- access token;
- refresh token;
- client id;
- client secret;
- native Reqwest error text containing URL data.

Custom providers returning `AuthError::with_safe_message` are responsible for the supplied text.

**SEC-PROV-001 — Provider isolation.** Provider request and response data MUST remain outside the application request observability pipeline.

## 11. Pagination data

Safe pagination errors may expose:

- endpoint;
- method;
- page index;
- error kind;
- configured hard cap.

**SEC-PAGE-002 — Pagination diagnostic exclusions.** Safe errors, recursive sources, structural `Debug`, hooks, request/response debug events, page completion, aggregate completion, and generated Rustdoc MUST NOT expose:

- cursor values;
- controller state;
- repeated logical URL;
- repeated query/header values;
- collected items.

**SEC-PAGE-001 — Continuation secrecy.** Declared bound pagination continuation values and repeated-request identities MUST NOT appear on automatic diagnostic or observation surfaces; their intentional use in ordinary request shaping, canonical identity, and selected-limiter inputs remains governed by those existing contracts.

**SEC-PAGE-003 — Bound-state continuation classification.** Only controller fields declared with `bind` MUST be classified as evolving pagination continuation state; stable unbound controller configuration MUST NOT be classified as continuation merely because the controller stores it.

## 12. Rate-limit keys

Custom limiters intentionally receive resolved rate-limit keys.

Those keys may contain private operational identifiers.

**SEC-RATE-002 — Rate-key diagnostic exclusion.** Built-in limiter errors and hooks MUST NOT reproduce resolved rate-key values.

A custom limiter is responsible for its own logs, metrics labels, errors, and storage.

**SEC-RATE-001 — Built-in key redaction.** Concord-provided limiter diagnostics MUST not reveal resolved key values.

**SEC-RATE-003 — Generated plans contain no request values.** Normal generated rate-limit plans,
limiter contexts, errors, hooks, completion events, generated Rustdoc, and public Rustdoc MUST NOT
contain client, scope, endpoint, host, method, pagination, body, credential, or other request values.
They MAY contain deterministic declaration IDs, static costs/windows, and documented global-key
status. Advanced explicit key parts remain available only to the intentionally selected custom
limiter and remain absent from built-in diagnostics and automatic output.

## 13. Hook and debug safety

Hooks and built-in debug output receive only:

- endpoint;
- method;
- safe URL;
- page and recovery indexes;
- sanitized headers;
- status where applicable;
- body category;
- known body length;
- safe terminal category.

They receive no:

- body bytes;
- credentials;
- native URL;
- raw transport error;
- provider data;
- cursor;
- rate-limit key.

**SEC-HOOK-001 — Metadata only.** Ordinary hook and debug inputs MUST remain metadata-only and sanitized.

## 14. Managed HTTP security

### 14.1 Redirects

**SEC-HTTP-003 — Redirects disabled.** Application and provider Reqwest clients MUST have redirect following disabled.

### 14.2 Cookies

**SEC-HTTP-004 — Cookies disabled.** Cookie storage and automatic cookie replay MUST be disabled.

### 14.3 Proxies

**SEC-HTTP-005 — System proxies disabled.** System proxy discovery MUST be disabled.

**SEC-HTTP-006 — Safe proxy restrictions.** A safe proxy MUST satisfy all of the following:

- use HTTP or HTTPS;
- contain a host;
- contain no user information;
- contain no path beyond `/`;
- contain no query;
- contain no fragment.

### 14.4 TLS

**SEC-HTTP-007 — No unsafe TLS bypass.** Safe configuration MUST NOT expose certificate-verification disablement.

Fixed HTTPS without TLS support fails construction.

Dynamic HTTPS without TLS support fails protected public request preparation before body or provider side effects.

An HTTP application endpoint is permitted only when the effective authentication plan is empty. Credential materialization onto cleartext HTTP fails during protected public preflight before hooks, body materialization, provider work, limiter acquisition, or native request construction.

Every provider HTTP request uses HTTPS. The ordinary provider API has no localhost or loopback cleartext exception.

**SEC-HTTP-008 — No cleartext application credentials.** An application request with an effective authentication plan MUST use HTTPS and MUST fail before protected side effects when its logical URL uses HTTP. The failure classification is `ErrorCategory::Configuration` with `ConfigurationErrorKind::InvalidAuthentication`.

**SEC-HTTP-009 — Provider HTTPS only.** Provider HTTP MUST reject every non-HTTPS URL before request-body factory invocation or native execution.


### 14.5 Client identities

mTLS identity is selected when constructing a managed client. Configured additional roots are
installed on both managed clients. The configured application identity is installed only on the
application client; the provider client does not present it. Concord 0.1 defines no
provider-specific client identity.

Concord 0.1 does not select client identities per request.

**SEC-HTTP-010 — TLS identity isolation.** The managed provider client MUST trust configured additional roots but MUST NOT receive or present the application identity configured by `identity_pem`.

**SEC-HTTP-001 — No implicit redirect/cookie/proxy.** Managed clients MUST disable redirects, cookie persistence, and implicit proxy discovery.

**SEC-HTTP-002 — No unsafe TLS API.** The safe configuration surface MUST provide no certificate-verification bypass.

## 15. Dangerous surfaces

### 15.1 Dangerous custom retry

The Reqwest retry builder and classifier may observe:

- native method;
- native status;
- native transport error;
- native materialized URL including query authentication.

Concord cannot sanitize application-owned logging or side effects inside that classifier.

### 15.2 Dangerous raw response

Raw response callers receive actual successful headers and body bytes.

They still receive the logical URL, not the native authenticated URL.

### 15.3 Development tools

Ordinary deterministic captures contain no body or credential values.

Explicit test-only fake-value expectations compare values internally and return redacted mismatch diagnostics.

**SEC-DANGER-001 — Reduced guarantees documented.** Every dangerous API MUST document the ordinary guarantee it reduces.

## 16. Fail-closed behavior

**SEC-FAIL-001 — Fail-closed configuration.** Concord MUST fail rather than silently weaken behavior when:

- auth placements collide;
- TLS capability is unavailable;
- public header ownership is invalid;
- a request body exceeds its limit;
- a response body exceeds its limit;
- a declared rate-limit plan lacks a functional or explicit no-op limiter;
- a rate-limit key exceeds its key limit;
- a pagination request identity repeats;
- checked arithmetic overflows.

**SEC-CLOSED-001 — Declared enforcement.** Declared authentication, body limits, rate limits, and pagination safety MUST NOT silently become no-op behavior.

**SEC-ROUTE-001 — Structural route containment.** Ordinary generated host and path values MUST be validated so one declared host atom cannot create another label or URL component and one declared path atom cannot create another segment or dot-segment traversal.

## 17. Default byte limits

The binary unit `MiB` is:

```text
1 MiB = 1,048,576 bytes
```

**BOUND-BODY-001 — Request default.** The default application request-body limit is 16 MiB for every generated or prepared `RequestBodySpec` and its one produced managed `RequestBody`.

**BOUND-BODY-002 — Buffered response default.** The default application buffered-response limit is 16 MiB for every `ResponseBodySpec<Mode = Buffered>` before `from_body` receives complete bytes.

**BOUND-BODY-003 — Stream response default.** The default application stream-response limit is 16 MiB for every `ResponseBodySpec<Mode = Streaming>` and its media-independent `ByteStream`.

**BOUND-BODY-004 — Provider request default.** The default provider request-body limit is 1 MiB.

**BOUND-BODY-005 — Provider response default.** The default provider response-body limit is 1 MiB.

**BOUND-BODY-009 — Terminal status-body default.** The default terminal non-success status-body capture limit is 64 KiB.

### 17.1 Request accounting

The request limit counts complete native data bytes.

For multipart, it includes framing, boundaries, part headers, and payload.

### 17.2 Response accounting

Response limits count bytes yielded by Reqwest after enabled decompression.

**BOUND-BODY-011 — Post-decompression response accounting.** Buffered acquisition, `ByteStream`, and record-stream cumulative response limits MUST count bytes yielded by Reqwest after enabled decompression.

### 17.3 Known and unknown lengths

Known excessive lengths fail before polling.

Unknown lengths fail before delivering the first excess frame or chunk.

### 17.4 Buffered codec peak allocation

The request limit bounds bytes returned by the codec and transmitted through Concord.

It does not guarantee that a custom synchronous codec's transient internal allocation stays below the limit.

**BOUND-CODEC-001 — Codec allocation awareness.** A codec receives the configured limit and SHOULD reject before excessive allocation when practical.

**BOUND-BODY-006 — Excess-frame withholding.** Concord MUST not retain, deliver to a response specification or record decoder, or decode the first request frame, response chunk, or record-staging append that causes its applicable limit to be exceeded.

### 17.5 Record staging

Record staging is bounded independently of the cumulative streaming response limit. Every
`RecordDecoder::max_buffered_bytes()` value is positive and finite. The built-in CSV and NDJSON
decoders use exactly 1 MiB as the maximum undecoded record or logical-line staging bound.

**BOUND-RECORD-001 — Positive finite decoder staging.** Every record decoder MUST declare a positive finite `max_buffered_bytes()` bound that is enforced independently of the cumulative `ByteStream` limit.

**BOUND-RECORD-002 — Record first-excess withholding.** The first append or completely framed record that exceeds the decoder staging bound MUST be rejected without retaining its offending bytes or yielding the oversized record.

**BOUND-RECORD-003 — Built-in record staging default.** `Csv<T>` and `NdJson<T>` MUST each use a 1 MiB positive finite maximum undecoded record or logical-line staging bound.

## 18. Finite limit configuration

Application request, buffered response, and stream response limits may be changed to another positive finite value.

Provider request and response limits may be changed client-wide to another positive finite value. A provider request may reduce its response limit but cannot increase it above the client-wide provider cap.

Terminal status-body capture may use another positive finite limit or may be disabled completely. Disabling capture discards terminal bodies and does not create an unbounded mode.

No ordinary Concord 0.1 API disables an application or provider success/request body limit. Zero is invalid for those limits.

**BOUND-BODY-007 — Finite ordinary limits.** Application request, buffered-response, stream-response, record-staging, provider-request, and provider-response limits MUST remain positive and finite through every generated, prepared, and ordinary advanced API.

**BOUND-BODY-008 — Provider cap.** A provider request MAY reduce its response limit but MUST NOT increase or disable the client-wide provider response cap.

**BOUND-BODY-010 — Finite terminal capture.** Terminal status-body capture MUST either be disabled or use a positive finite limit; no ordinary or dangerous API may make terminal error-body capture unbounded.

## 19. Authentication bounds

**BOUND-AUTH-001 — Refresh skew.** Default proactive credential refresh skew is 60 seconds.

**BOUND-AUTH-002 — Failure cooldown cap.** Default maximum provider failure cooldown is 60 seconds.

The application may configure a different finite cooldown cap, including zero.

A provider-supplied cooldown above the cap is clamped.

**BOUND-AUTH-003 — Recovery count.** A protected endpoint or page operation permits at most one additional authentication attempt.

**BOUND-AUTH-004 — One slot operation.** One credential slot permits at most one active acquisition or refresh operation.

**BOUND-AUTH-005 — Bounded history.** A credential slot MUST not retain an unbounded history of previous values or versions.

## 20. Rate-limit bounds

**BOUND-RATE-001 — Cooldown cap.** Default maximum server-directed rate-limit cooldown is 60 seconds.

**BOUND-RATE-002 — Quota entries.** Default built-in quota-entry capacity is 4096.

**BOUND-RATE-003 — Cooldown entries.** Default built-in cooldown-entry capacity is 4096.

**BOUND-RATE-004 — Idle lifetime.** Default idle quota-entry lifetime is 15 minutes.

**BOUND-RATE-005 — Key bytes.** Default maximum aggregate byte length of one resolved rate-limit key is 1024 bytes.

The governor may expose configuration for these finite values.

A configured entry capacity of zero is invalid.

A configured key limit of zero permits only an empty key.

The built-in limiter may evict least-recently-used local state to preserve capacity. Eviction may forget local quota history and may permit a fresh local burst for an evicted key. The governor is bounded best-effort process-local pacing, not strict or distributed quota enforcement.

**BOUND-RATE-006 — Finite state.** Built-in quota and cooldown maps MUST remain within configured entry capacities.

**BOUND-RATE-007 — Best-effort governor.** Built-in state eviction MAY forget local quota history; documentation MUST describe the governor as bounded best-effort process-local pacing rather than strict enforcement.

## 21. Pagination bounds

**BOUND-PAGE-001 — Explicit finite limit.** Every pagination operation MUST receive a finite `PageLimit`.

**BOUND-PAGE-002 — One active page.** One pagination operation MUST have at most one active page request.

**BOUND-PAGE-003 — Identity count.** The pagination request-identity set MUST contain at most one entry per prepared distinct page and is dropped at operation completion.

**BOUND-PAGE-004 — Default page ceiling.** The default maximum requested page count is 1,000 pages.

**BOUND-PAGE-005 — Default item ceiling.** The default maximum collected item count is 100,000 items.

**BOUND-PAGE-006 — Default identity-storage ceiling.** The default maximum retained pagination request-identity storage is 4 MiB.

The application may configure different positive finite pagination ceilings.

A nonzero requested `PageLimit` above the configured page ceiling fails before controller construction or endpoint planning.

Pagination fails before appending an item that would exceed the configured item ceiling. It fails before retaining a request identity that would exceed the configured identity-storage ceiling. Either failure returns no partial collection.

These ceilings bound page count, item count, and retained identity bytes. They do not guarantee bounded decoded object memory, because an application-defined item may retain arbitrary internal allocation. They also do not define a whole-collection elapsed-time deadline.

Low-memory incremental item/page processing remains outside Concord 0.1.

**BOUND-PAGE-007 — Preflight page ceiling.** A nonzero requested page limit MUST NOT exceed the configured page ceiling and MUST be rejected before controller construction when it does.

**BOUND-PAGE-008 — Aggregate item ceiling.** Pagination MUST fail before appending an item that would exceed the configured collected-item ceiling and MUST return no partial collection.

**BOUND-PAGE-009 — Identity byte ceiling.** Pagination MUST fail closed before retaining an identity that would exceed the configured identity-storage ceiling and MUST return no partial collection.

**BOUND-PAGE-010 — Canonical identity byte measure.** The identity-storage ceiling MUST count canonical endpoint-identity, method, logical-URL, public-header-name, and public-header-value bytes as defined by `EXEC-PAGE-013`, rather than implementation allocator overhead.

## 22. Compile-time bounds

**BOUND-COMPILE-001 — Scope depth.** Maximum nested DSL scope depth is 64.

**BOUND-COMPILE-002 — Format depth.** Maximum nested `fmt[...]` depth is 64.

Exceeding a compile-time bound must produce a controlled diagnostic.

## 23. Arithmetic

**BOUND-MATH-001 — Safe arithmetic.** Time, length, page index, offset, page-number, quota, and count calculations MUST use checked or saturating behavior appropriate to the contract.

**BOUND-MATH-002 — No observable wraparound.** Observable counters and deadlines MUST NOT wrap.

Overflow becomes a typed safe error.


## 24. Callback and extension bounds

Concord does not impose an automatic timeout or memory cap on custom:

- hooks;
- codecs;
- request and response specifications;
- record decoders and batching consumers;
- credential providers;
- body factories;
- limiters;
- rate-limit observers;
- pagination controllers;
- dangerous retry callbacks.

The operation deadline still bounds result-determining asynchronous extensions such as
`before_send`, `on_response_head`, provider work, and limiter waiting. `on_complete` and
`on_pagination_complete` begin only after the result and elapsed duration are fixed and are awaited
outside that deadline. They are not detached, may delay delivery of the fixed result, and may be
prevented from completing by cancellation or panic.

**SEC-EXT-001 — Extension-owned boundedness.** Extension authors MUST bound their own:

- execution time;
- retries;
- retained maps and queues;
- task creation;
- logging volume;
- metric cardinality;
- external I/O.

This responsibility applies in particular to completion hooks because Concord applies no operation
deadline or separate completion timeout after result fixation.

**BOUND-EXT-001 — Extension responsibility.** A stable extension contract MUST document concurrency, cancellation, sensitivity, and boundedness obligations.

## 25. Safe diagnostic matrix

| Surface | Credentials | Request body | Response body | Provider body | Cursor | Rate key | Native auth URL |
|---|---:|---:|---:|---:|---:|---:|---:|
| `ApiError` display/debug | No | No | No | No | No | No | No |
| Concord-controlled source chain | No | No | No | No | No | No | No |
| Before-send hook | No | No | No | No | No | No | No |
| Response-head hook | No | No | No | No | No | No | No |
| Completion hook | No | No | No | No | No | No | No |
| Built-in debug output | No | No | No | No | No | No | No |
| Rate-limit request/response context | No | No | No | No | No | Yes | No |
| Built-in rate-limit error | No | No | No | No | No | No | No |
| Pagination error | No | No | No | No | No | No | No |
| Generated Rustdoc | No | No | No | No | No | No | No |
| Wrapper `Debug` | No auth | No | No body/value | No | No | No | No |

The custom limiter's intentional rate-key access is the only `Yes` in ordinary advanced callback data.

## 26. Security sentinel classes

Conformance testing must include unique sentinels for:

- API key;
- bearer token;
- Basic username;
- Basic password;
- OAuth client id and secret;
- OAuth access token;
- query auth value;
- sensitive public query value;
- auth header;
- request body;
- multipart text, bytes, and filename;
- response body;
- sensitive successful response header;
- non-success response body;
- provider URL/body/response;
- cursor and controller state;
- rate-limit key;
- proxy URL;
- native authenticated URL.

**SEC-TEST-001 — Cross-surface sentinel coverage.** Every applicable sentinel class MUST be checked across errors, debug, hooks, source chains, limiter contexts, pagination diagnostics, and wrapper debugging.
