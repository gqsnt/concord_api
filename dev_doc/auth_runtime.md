# Auth runtime

Auth is declared by the macro and executed by `concord_core`.

## Inputs and redaction

Auth vars and secrets are generated client inputs. Secret values are wrapped and redacted. Errors and diagnostics should identify credentials, headers, or fields by name without rendering raw secret values.

Runtime debug/display output must not render header auth values, bearer tokens, Basic auth usernames or passwords declared as secrets, OAuth client secrets, or query-auth values. Debug sinks and hooks also do not receive live request or response body bytes, so auth/token endpoint bodies cannot be previewed through diagnostics. The materialized transport request still carries the real credential material required by the remote API; redaction is only for diagnostics, debug output, generated docs, and derived display/cache/debug keys.

## Credentials

Credential declarations create providers and credential slots. Static providers include API key, bearer token, basic credentials, and OAuth2 client credentials. Endpoint-backed credentials are populated by executing an auth endpoint and mapping its decoded response into credential material. In code and tests, endpoint-backed credentials are the primary stateful auth example.

Credential slots store material and monotonic generation counters. Every slot
state, including empty and failed states, preserves a generation so the same
slot never reuses an older epoch during a client auth-state lifetime.

## Auth state

Generated auth state accessors expose explicit checks and clearing. Endpoint-backed credentials can be acquired manually with generated acquisition methods.

Protected calls that depend on endpoint-backed credentials fail before transport when the credential slot is empty. Rejection/refresh handling only applies after credential material has been acquired and applied to a request.

Generated helpers that observe or mutate auth state are fallible when they touch shared auth locks. Endpoint-backed `set`, `clear`, and `is_set` helpers return `AuthError` on state-unavailable failures instead of panicking. Request execution maps auth-state lock failure into `ApiClientError::Auth`.

## Request auth application

Before cache identity is computed, the runtime resolves required credentials and attaches typed pending auth slots to the logical `BuiltRequest`. No auth application hook receives `BuiltRequest`: endpoint auth preparation and auth-internal preparation both receive an auth-only application request. That request can attach pending auth slots and mark auth query keys as sensitive, but it cannot mutate the logical URL, headers, body, timeout, retry, cache, rate-limit, or metadata. Custom `ClientContext` implementations must use the core `apply_*_credential` helpers instead of writing auth values into request headers or query strings.

A pending slot records the placement, credential id, usage id, generation, provenance, and safe identity. It does not store the raw secret.

Raw credential material is kept in a short-lived per-attempt sidecar and is inserted only when the runtime materializes a `TransportRequest` immediately before `Transport::send`. `BuiltRequest`, `BuiltResponse`, `DecodedResponse<T>`, cache keys, runtime hooks, and debug sinks must never store raw auth material.

Safe identities are used for cache separation. They identify credential state without exposing secret values. Basic credentials use opaque fingerprints of the full Basic credential state by default, including both the secret username and the secret password; readable identity hints must be explicitly supplied as non-secret labels by advanced integrations.

Protected cache identity is built from pending auth slots, not from
materialized auth headers or query values. The default key includes safe
credential metadata, placement, generation when known, and safe identity. This
keeps query-auth credentials from colliding with the public URL key while still
keeping raw query-auth values out of cache keys and diagnostics. If an auth
requirement resolves only to anonymous identity, request execution bypasses
cache lookup/store/fallback for that protected request.

Page and custom pagination mutation happens before auth-collision validation,
cache lookup, rate-limit acquisition, and transport materialization. The
runtime uses the final mutated logical request as the input to safe metadata
construction, then materializes raw auth only into `TransportRequest`.

Query-auth materialization must reject a public query parameter that already
uses the auth query key. The rejection happens before raw query-auth material is
appended and before cache lookup, rate-limit acquisition, and transport send,
and the typed error may name the key but must not include the secret value.

Header-auth materialization follows the same structural rule: public headers
cannot silently collide with bearer, Basic, or custom auth headers, and header
matching is case-insensitive. The runtime rejects those collisions before cache
lookup, rate-limit acquisition, and transport rather than overwriting the
public header value.

Custom transports receive the materialized `TransportRequest`, so they see real credentials at the send boundary. Transport implementations must not log the raw request.

## Rejection and refresh

Auth rejection handling runs after response classification but before normal retry. If configured, the runtime can invalidate rejected credential material and perform bounded auth refresh before retrying the protected request for credentials the runtime can reacquire.

The v1 default policy is:

- `401 Unauthorized`: invalidate the applied credential and retry after refresh for refreshable/reacquirable runtime credentials.
- `403 Forbidden`: invalidate the applied credential and retry after refresh for refreshable/reacquirable runtime credentials.

Endpoint-backed credentials are manual from the protected request's point of view. A protected `401` can invalidate the applied endpoint-backed generation, but protected request retry does not automatically call the auth endpoint again; users must explicitly reacquire through the auth endpoint before sending another protected call.

Protected auth rejection is handled before ordinary retry and does not fall back to stale cached data by default. Runtime integrations can still narrow forbidden handling by customizing `AuthStepPolicy`, but the v1 default treats both `401` and `403` as protected auth rejection statuses.

Credential refresh is bounded by the client runtime `max_auth_retries` setting. The runtime must not loop indefinitely on repeated auth rejection.

`AuthChallengePolicy::NeverRefresh` is part of the advanced core API. When a requirement uses it, auth rejection does not invalidate, retry, or stale-fallback for `401` or `403`. It is not exposed as public DSL syntax in v1.

Credential slots carry monotonic generation counters. Invalidating a rejected
credential targets the generation that was applied to the failed request, so
stale invalidation cannot clear newer credential material that was acquired
after the request was sent. Credential acquisition and refresh transition the
slot into an in-flight generation while the auth lock is not held across network
I/O. Completion stores the result only if the slot is still in that in-flight
generation; stale completions are discarded. If the acquiring future is dropped
or cancelled, the in-flight guard rolls the slot forward to a safe state and
wakes waiters instead of leaving the slot permanently in flight.

Auth-internal requests use recursion guards so an auth refresh request does not recursively trigger the same auth flow.

Auth-internal HTTP responses are also bounded. Token and credential-acquisition responses use `AuthInternalPolicy::max_body_bytes`, which defaults to 1 MiB. The auth executor checks `Content-Length` before reading when present and enforces the same limit while reading unknown-length bodies. Oversized auth responses return `AuthErrorKind::ResponseTooLarge`; they are not treated as retryable transport read failures by default.

## Advanced forms

Certificate auth is an attachment form for `ClientCertificate` material. The DSL does not provide a secret-derived certificate constructor in v1; certificate material must come from endpoint-backed or runtime-provided credential material.

OAuth2 client credentials are represented as a credential provider that fetches and refreshes bearer access tokens at a high level. Generated clients configure the provider from `oauth2_client { token_url, client_id, client_secret, scope? }`. Acquisition sends `POST` to `token_url` with HTTP Basic client authentication, form body `grant_type=client_credentials`, and optional `scope`. A successful token response becomes `AccessToken` material, is stored in the credential slot, and is materialized as `Authorization: Bearer ...` only when the protected `TransportRequest` is built.

OAuth token reuse, cancellation safety, and protected `401` refresh use the same `CredentialSlot` path as other refreshable credentials. Token endpoint failure returns an auth error and blocks the protected request from being sent. OAuth client secrets and tokens remain redacted from debug/errors.
