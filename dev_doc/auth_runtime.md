# Auth runtime

Auth is declared by the macro and executed by `concord_core`.

## Inputs and redaction

Auth vars and secrets are generated client inputs. Secret values are wrapped and redacted. Errors and diagnostics should identify credentials, headers, or fields by name without rendering raw secret values.

Runtime debug/display output must not render header auth values, bearer tokens, Basic auth usernames or passwords declared as secrets, OAuth client secrets, or query-auth values. The materialized transport request still carries the real credential material required by the remote API; redaction is only for diagnostics, debug output, generated docs, and derived display/cache/debug keys.

## Credentials

Credential declarations create providers and credential slots. Static providers include API key, bearer token, basic credentials, and OAuth2 client credentials. Endpoint-backed credentials are populated by executing an auth endpoint and mapping its decoded response into credential material. In code and tests, endpoint-backed credentials are the primary stateful auth example.

Credential slots store material and generation counters. Generations let the runtime identify whether a credential was refreshed or invalidated between attempts.

## Auth state

Generated auth state accessors expose explicit checks and clearing. Endpoint-backed credentials can be acquired manually with generated acquisition methods.

Protected calls that depend on endpoint-backed credentials fail before transport when the credential slot is empty. Rejection/refresh handling only applies after credential material has been acquired and applied to a request.

Generated helpers that observe or mutate auth state are fallible when they touch shared auth locks. Endpoint-backed `set`, `clear`, and `is_set` helpers return `AuthError` on state-unavailable failures instead of panicking. Request execution maps auth-state lock failure into `ApiClientError::Auth`.

## Request auth application

Before cache identity is computed, the runtime resolves required credentials and attaches typed pending auth slots to the logical `BuiltRequest`. No auth application hook receives `BuiltRequest`: endpoint auth preparation and auth-internal preparation both receive an auth-only application request. That request can attach pending auth slots and mark auth query keys as sensitive, but it cannot mutate the logical URL, headers, body, timeout, retry, cache, rate-limit, or metadata. Custom `ClientContext` implementations must use the core `apply_*_credential` helpers instead of writing auth values into request headers or query strings.

A pending slot records the placement, credential id, usage id, generation, provenance, and safe identity. It does not store the raw secret.

Raw credential material is kept in a short-lived per-attempt sidecar and is inserted only when the runtime materializes a `TransportRequest` immediately before `Transport::send`. `BuiltRequest`, `BuiltResponse`, `DecodedResponse<T>`, cache keys, runtime hooks, and debug sinks must never store raw auth material.

Safe identities are used for cache separation. They identify credential state without exposing secret values. Basic credentials use opaque fingerprints of the full Basic credential state by default, including both the secret username and the secret password; readable identity hints must be explicitly supplied as non-secret labels by advanced integrations.

Custom transports receive the materialized `TransportRequest`, so they see real credentials at the send boundary. Transport implementations must not log the raw request.

## Rejection and refresh

Auth rejection handling runs before normal retry. If configured, the runtime can invalidate rejected credential material and perform bounded auth refresh before retrying the protected request for credentials the runtime can reacquire.

The v1 default policy is:

- `401 Unauthorized`: invalidate the applied credential and retry after refresh for refreshable/reacquirable runtime credentials.
- `403 Forbidden`: do not invalidate and do not retry.

Endpoint-backed credentials are manual from the protected request's point of view. A protected `401` can invalidate the applied endpoint-backed generation, but protected request retry does not automatically call the auth endpoint again; users must explicitly reacquire through the auth endpoint before sending another protected call.

The default `403` behavior is deliberate: a forbidden response usually means the credential was accepted but lacks permission. Runtime integrations can opt into forbidden invalidation/retry by using `AuthStepPolicy` directly.

Credential refresh is bounded by the client runtime `max_auth_retries` setting. The runtime must not loop indefinitely on repeated auth rejection.

`AuthChallengePolicy::NeverRefresh` is part of the advanced core API. When a requirement uses it, auth rejection does not invalidate or retry for `401` or `403`. It is not exposed as public DSL syntax in v1.

Credential slots carry generation counters. Invalidating a rejected credential should target the generation that was applied to the failed request, so stale invalidation cannot clear newer credential material that was acquired after the request was sent.

Auth-internal requests use recursion guards so an auth refresh request does not recursively trigger the same auth flow.

Auth-internal HTTP responses are also bounded. Token and credential-acquisition responses use `AuthInternalPolicy::max_body_bytes`, which defaults to 1 MiB. The auth executor checks `Content-Length` before reading when present and enforces the same limit while reading unknown-length bodies. Oversized auth responses return `AuthErrorKind::ResponseTooLarge`; they are not treated as retryable transport read failures by default.

## Advanced forms

Certificate auth is an attachment form for `ClientCertificate` material. The DSL does not provide a secret-derived certificate constructor in v1; certificate material must come from endpoint-backed or runtime-provided credential material.

OAuth2 client credentials are represented as a credential provider that fetches and refreshes bearer access tokens at a high level. The runtime handles token acquisition through the provider before applying bearer auth.
