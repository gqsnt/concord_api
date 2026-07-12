# Auth Runtime

Auth is declared by the macro and executed by `concord_core`.

## Inputs And Redaction

Auth vars and secrets are generated client inputs. Secret values are wrapped and redacted. Errors and diagnostics should identify credentials, headers, or fields by name without rendering raw secret values.

Runtime debug and display output must not render header auth values, bearer tokens, Basic auth usernames or passwords declared as secrets, OAuth client secrets, or query-auth values. Debug sinks and hooks receive sanitized metadata views, not raw header maps, and they do not receive live request or response body bytes, so auth and token endpoint bodies cannot be previewed through diagnostics. The materialized transport request still carries the real credential material required by the remote API; redaction is only for diagnostics, debug output, and generated docs.

## Credentials

Credential declarations create providers and credential slots. Static providers include API key, bearer token, Basic credentials, and OAuth2 client credentials. Endpoint-backed credentials are populated by executing an auth endpoint and extracting credential material from its decoded response.

Credential slots store material and monotonic generation counters. Every slot state, including empty and failed states, preserves a generation so the same slot never reuses an older epoch during a client auth-state lifetime.

## Auth State

Generated auth-state accessors expose explicit checks and clearing. Endpoint-backed credentials can be acquired manually with generated acquisition methods.

Protected calls that depend on endpoint-backed credentials fail before transport when the credential slot is empty. Rejection and refresh handling only applies after credential material has been acquired and applied to a request.

Generated helpers that observe or mutate auth state are fallible when they touch runtime auth locks. Endpoint-backed `set`, `clear`, and `is_set` helpers return `AuthError` on state-unavailable failures instead of panicking. Request execution maps auth-state lock failure into `ApiClientError::Auth`.

Cloned clients share auth state. Runtime configuration uses clone-on-write, but auth-state accessors, `set`, `clear`, `is_set`, and endpoint-backed acquisition operate on a shared auth-state handle. Clearing or replacing auth state on one clone affects the other clones that share that handle. Code that needs credential isolation should build a separate client instance or install separate auth state explicitly; `vars` and `auth_vars` cloning do not isolate credentials.

## Request Auth Application

Before provider invocation, the runtime derives a secret-free placement plan from resolved `AuthRequirement` values, registers sensitive query keys, and validates it against the public URL and headers. No auth application hook receives `BuiltRequest`: endpoint auth preparation and auth-internal preparation receive an auth-only application request already bound to a planned slot. It can bind compatible material but cannot create or change placement, add a header or query key, or mutate request metadata. Custom `ClientContext` implementations must use the core `apply_*_credential` helpers.

A planned slot records placement, credential and usage identity, step identity, provenance, and a request-local binding identity. Credential generation remains preparation output rather than placement state. Neither structure stores raw credential material.

Raw credential material is kept in a short-lived per-attempt sidecar and is inserted only when the runtime materializes a `http::Request<DynBody>` immediately before `Transport::send`. `BuiltRequest`, `BuiltResponse`, `DecodedResponse<T>`, runtime hooks, and debug sinks must never store raw auth material. Hook and debug metadata redaction applies before callback invocation, so sensitive headers and query values are not exposed through those surfaces.

Page and custom pagination mutation happens before auth-collision validation, rate-limit acquisition, and transport materialization. The runtime uses the final mutated logical request as the input to safe metadata construction, then materializes raw auth only into `http::Request<DynBody>`.

Query-auth preflight rejects a public query parameter that already uses the auth query key before provider invocation or body production. The typed error may name the key but must not include the public value, complete URL, or secret value.

Header-auth preflight follows the same structural rule: public headers cannot silently collide with bearer, Basic, or custom auth headers, and header matching is case-insensitive. Custom `Authorization` placement shares the bearer/Basic singleton target. Malformed and duplicate runtime placement plans fail before providers.

Custom transports receive the materialized `http::Request<DynBody>`, so they see real credentials at the send boundary. Transport implementations must not log the raw request.

## Rejection And Refresh

Auth rejection handling runs after response classification but before normal retry. If configured, the runtime can invalidate rejected credential material and perform bounded auth refresh before retrying the protected request for credentials the runtime can reacquire.

The v1 rejection classification is:

- `401 Unauthorized`: invalidate the applied credential and request refresh for refreshable or runtime-reacquirable credentials when the request has replayable body capacity remaining.
- `403 Forbidden`: invalidate the applied credential and request refresh for refreshable or runtime-reacquirable credentials when the request has replayable body capacity remaining.

The effective absolute attempt cap controls whether the refresh resend can occur. Its default is `max_attempts = 1`, so the default permits no resend; there is no separate authentication retry budget.

Endpoint-backed credentials are manual from the protected request's point of view. A protected `401` can invalidate the applied endpoint-backed generation, but protected request retry does not automatically call the auth endpoint again; callers must explicitly reacquire through the auth endpoint before sending another protected call.

Credential refresh resends use the protected request’s absolute `max_attempts` cap. The runtime must not loop indefinitely on repeated auth rejection, and authentication does not have a separate retry ceiling.

`AuthChallengePolicy::NeverRefresh` is part of the advanced core API. When a requirement uses it, auth rejection does not invalidate or retry for `401` or `403`. It is not exposed as public DSL syntax in v1.

`AuthStepPolicy` is still the v1 bool matrix:

| retry | invalidate | Meaning |
| --- | --- | --- |
| `true` | `true` | Default path. Invalidate the applied generation and retry if the credential can be reacquired. |
| `true` | `false` | Retry without clearing the applied generation first. The provider may still return the same material. |
| `false` | `true` | Invalidate the applied generation, then return a terminal auth rejection for the protected request. |
| `false` | `false` | Terminal auth rejection with no invalidation and no retry. |

Credential slots carry monotonic generation counters. Invalidating a rejected credential targets the generation that was applied to the failed request, so an older invalidation cannot clear newer credential material that was acquired after the request was sent. Credential acquisition and refresh transition the slot into an in-flight generation while the auth lock is not held across network I/O. Completion stores the result only if the slot is still in that in-flight generation; older completions are discarded. If the acquiring future is dropped or cancelled, the in-flight guard rolls the slot forward to a safe state and wakes waiters instead of leaving the slot permanently in flight.

Auth-internal requests use recursion guards so an auth refresh request does not recursively trigger the same auth flow.

Auth-internal HTTP responses are also bounded. Token and credential-acquisition responses use `AuthInternalPolicy::DEFAULT_MAX_BODY_BYTES` as their default read limit. The auth executor checks `Content-Length` before reading when present and enforces the same limit while reading unknown-length bodies. Oversized auth responses return `AuthErrorKind::ResponseTooLarge`; they are not treated as retryable transport read failures by default.

## Advanced Forms

OAuth2 client credentials are represented as a credential provider that fetches and refreshes bearer access tokens at a high level. Generated clients configure the provider from `oauth2_client { token_url, client_id, client_secret, scope? }`. Acquisition sends `POST` to `token_url` with HTTP Basic client authentication, form body `grant_type=client_credentials`, and optional `scope`. A successful token response becomes `AccessToken` material, is stored in the credential slot, and is materialized as `Authorization: Bearer ...` only when the protected `http::Request<DynBody>` is built.

OAuth token reuse, cancellation safety, and protected `401` refresh use the same `CredentialSlot` path as other refreshable credentials. Token endpoint failure returns an auth error and blocks the protected request from being sent. OAuth client secrets and tokens remain redacted from debug output and errors.
