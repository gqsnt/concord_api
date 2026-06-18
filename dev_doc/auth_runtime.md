# Auth runtime

Auth is declared by the macro and executed by `concord_core`.

## Inputs and redaction

Auth vars and secrets are generated client inputs. Secret values are wrapped and redacted. Errors and diagnostics should identify credentials, headers, or fields by name without rendering raw secret values.

Runtime debug/display output must not render header auth values, bearer tokens, basic passwords, OAuth client secrets, or query-auth values. The transport request still carries the real credential material; redaction is only for diagnostics, debug output, generated docs, and derived display/cache/debug keys.

## Credentials

Credential declarations create providers and credential slots. Static providers include API key, bearer token, basic credentials, and OAuth2 client credentials. Endpoint-backed credentials are populated by executing an auth endpoint and mapping its decoded response into credential material. In code and tests, endpoint-backed credentials are the primary stateful auth example.

Credential slots store material and generation counters. Generations let the runtime identify whether a credential was refreshed or invalidated between attempts.

## Auth state

Generated auth state accessors expose explicit checks and clearing. Endpoint-backed credentials can be acquired manually with generated acquisition methods.

Protected calls that depend on endpoint-backed credentials fail before transport when the credential slot is empty. Rejection/refresh handling only applies after credential material has been acquired and applied to a request.

## Request auth application

Before cache and inflight identity are computed, the runtime resolves required credentials and applies auth to the request. This ordering prevents authenticated requests from colliding across different credential identities.

Safe identities are used for cache and inflight separation. They identify credential state without exposing secret values.

## Rejection and refresh

Auth rejection handling runs before normal retry. If configured, the runtime can invalidate rejected credential material and perform bounded auth refresh before retrying the protected request.

The v1 default policy is:

- `401 Unauthorized`: invalidate the applied credential and retry after refresh.
- `403 Forbidden`: do not invalidate and do not retry.

The default `403` behavior is deliberate: a forbidden response usually means the credential was accepted but lacks permission. Runtime integrations can opt into forbidden invalidation/retry by using `AuthStepPolicy` directly.

Credential refresh is bounded by the client runtime `max_auth_retries` setting. The runtime must not loop indefinitely on repeated auth rejection.

`AuthChallengePolicy::NeverRefresh` is part of the advanced core API. When a requirement uses it, auth rejection does not invalidate or retry for `401` or `403`. It is not exposed as public DSL syntax in v1.

Credential slots carry generation counters. Invalidating a rejected credential should target the generation that was applied to the failed request, so stale invalidation cannot clear newer credential material that was acquired after the request was sent.

Auth-internal requests use recursion guards so an auth refresh request does not recursively trigger the same auth flow.

## Advanced forms

Certificate auth is an attachment form for `ClientCertificate` material. The DSL does not provide a secret-derived certificate constructor in v1; certificate material must come from endpoint-backed or runtime-provided credential material.

OAuth2 client credentials are represented as a credential provider that fetches and refreshes bearer access tokens at a high level. The runtime handles token acquisition through the provider before applying bearer auth.
