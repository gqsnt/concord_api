# Security and redaction

Concord keeps credential placement and native application transport inside the
managed runtime. Public shaping, planning, hooks, limiter admission, and
ordinary diagnostics operate on semantic values that exclude private
authentication material.

Redaction reduces accidental disclosure. It does not classify public
application data as safe.

## Private request boundary

The request path separates public and private work:

```text
bounded public URL/header shaping
→ safe structural plan and before-send event
→ managed body preparation
→ credential preflight and limiter admission
→ private authentication placement
→ native send
```

The DSL accepts secrets and credentials only in `credentials` and `auth`
clauses. Public `host`, `path`, `query`, and `headers` expressions accept
restricted public references and reject `secret.*` and `credential.*`.

Bearer and Basic placements exclusively own `Authorization`. Generic private
header placement rejects that header; private query placement remains
supported. Collision checks prevent public and private owners from writing the
same target.

Credential material is exposed only at the private placement boundary.
Providers receive bounded isolated `ProviderHttp`, not the application client
or request. Hooks and limiters run before private header/query mutation and
cannot inspect the authenticated native request.

## Observation surfaces

Different public values have different disclosure contracts:

| Surface | Intentionally available | Excluded or suppressed |
|---|---|---|
| `EndpointDescription` | stable identity, route/body/response categories, timeout and capability flags | bound values, bodies, credentials, resolved/detailed rule keys, runtime |
| `SafeRequestPlan` | final public method, logical URL, public headers, timeout/body shape | hooks, credentials, limiter effects, encoded body, native request |
| `BeforeSend` | public head, operation/page IDs, finite `AttemptInfo`, body kind and known length | body bytes, private auth, native transport |
| `ResponseHeadEvent` | status and sanitized response headers | body and protected header values |
| rate limiter request | safe logical URL plus complete resolved rate plan | credentials, body, native request |
| rate observer input | sanitized response head plus resolved plan | response body and native response |
| `ApiError` formatting | endpoint, method, categories, focused safe kinds, status | native causes, protected values, body bytes, header values |
| generated request `Debug` | safe structural type/state | body input and secrets |
| completion events | IDs, category, status, bounded timing/counts | bodies, cursors, credentials, native diagnostics |

The selected limiter is a correctness authority and receives resolved
rate-limit keys. Those keys cannot contain DSL secrets or credentials, but
tenant, account, region, host, and endpoint values can still be operationally
sensitive. `RateLimitKey` and `RateLimitPlan` have no `Debug` or `Display`.
Concord does not copy resolved values into errors, hooks, completion,
descriptions, plans, or generated Rustdoc. A custom limiter or observer owns
equivalent protection in its storage and logs.

## Credential-sanitized is not anonymized

Plans, hooks, limiters, completion events, and many errors use a
credential-sanitized logical URL. Private authentication query values are
absent. This URL may still include:

- account, tenant, user, or resource IDs;
- names and search terms;
- regional routing values;
- public API keys that the application incorrectly modeled as public;
- other personal or business data.

Sanitized headers likewise remove protected authentication placements but can
retain public service metadata. Apply an application data policy before
logging either value. Prefer stable endpoint IDs and bounded outcome categories
for metrics; do not use full URLs, operation IDs, cursors, or arbitrary header
values as labels.

Planning is not an exact request preview. It executes real public head shaping,
then stops before hooks, body encoding/factories, credentials, limiter
admission, native transport, and completion. Concord intentionally provides no
API for previewing the final authenticated request.

## Successful responses and trusted specifications

An ordinary `ApiResponse<T>` owns the actual successful response headers and
decoded application value. Its `Debug` output omits header values and the
application value, but explicit accessors return them. Treat both as
application data.

A `ResponseBodySpec` receives actual accepted headers and a bounded semantic
body (`Bytes`, `ByteStream`, or `()`) after core-owned acquisition. A
`RecordDecoder` receives response-head metadata and staged response bytes.
These are trusted extension boundaries, not automatically safe logging
surfaces.

Byte and record stream failures are redacted, but successfully yielded bytes
or records belong to the application. Dropping a stream cancels later polling;
it does not erase values already retained downstream.

## Errors and terminal status bodies

`ApiError`, nested safe subsystem errors, and build errors retain stable
categories rather than native causes. Parser errors, Reqwest errors, provider
payloads, credential values, body producer errors, and hook messages are
dropped or reduced according to the [Error reference](../reference/errors.md).

A terminal `HttpResponseFailure` can own a complete bounded `StatusBody`.
Formatting exposes only its structural state and length. Explicit
`StatusBody::bytes`, `into_bytes`, or `decode_body<C>` access can reveal the
retained response content; that is an intentional application action and must
follow the response data policy.

Application-approved safe messages on authentication, pagination, or limiter
errors remain trusted text. Concord cannot verify or universally bound every
such string. Exclude credentials, URLs, keys, headers, cursors, body content,
parser output, and backend diagnostics.

## Credential storage and providers

`SecretString`, `AccessToken`, `ApiKey`, and `BasicCredential` have redacted
formatting and finite construction limits. Cloning a `SecretString` shares its
immutable allocation, but this is not zeroization or a general-purpose secret
store. Load inputs through an approved application mechanism and control their
lifetime.

Custom `CredentialMaterial` owns safe `Debug`/`Display`, memory retention, and
destruction. `SecretCredential::expose_secret` is explicit because it crosses
the private placement boundary.

Provider URLs, request bodies, response bodies, and current material may all be
sensitive. Provider HTTP is isolated and bounded, but a provider's own logs,
caches, retries, and external systems remain outside Concord's redaction
control.

`response_bytes()` returns `ApiResponse<Bytes>` and intentionally makes the
actual accepted headers and complete bounded body available through explicit
accessors. Its `Debug` output omits the logical URL, header values, and body
value. Once accessed, those values are application-owned data subject to the
same policy as decoded output.

## Dangerous capabilities

A custom native retry classifier can observe native materialized URLs,
including private query authentication.

That value does not appear in Concord's automatic diagnostics. Application
code inside this capability owns confidentiality. See
[Dangerous capabilities](../reference/dangerous.md) before enabling the
feature.

## Verification

Use unique sentinels in tests and search every formatting and observation
surface recursively. Cover plans, descriptions, generated request `Debug`,
hooks, completion, errors and their sources, rate metadata, provider failures,
terminal status states, and dangerous-capability formatting.

Also test the positive boundary: private authentication must reach the
controlled loopback server even though it is absent from the public plan and
hooks. A test that checks only absence can accidentally verify a client that
never authenticated.

## Related

- [Authentication](../features/authentication.md)
- [Requests, planning, and errors](../guide/requests-planning-errors.md)
- [Hooks and observability](../features/hooks-and-observability.md)
- [Error reference](../reference/errors.md)
- [Finite behavior](finite-behavior.md)
- [Dangerous capabilities](../reference/dangerous.md)
