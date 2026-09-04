# Authentication

Authentication converts application authority into bounded private material, stores it across time, attributes rejection to the correct source, and places it at the final native boundary.

## Conceptual pipeline

```text
protected construction input
→ credential source/provider
→ versioned credential slot
→ current protected material
→ private placement
→ challenge attribution
→ source-specific invalidation/refresh
→ at most one recovery replay
```

These are separate concepts and must not be collapsed into a header setter.

## Declare sources

```concord-pseudocode
credentials {
    secret bootstrap_key;
    secret optional_api_key?;

    credential bootstrap = api_key(secret.bootstrap_key);
    credential optional_key = api_key(secret.optional_api_key);
    credential oauth = custom OAuthClientCredentials;
    credential session = endpoint authenticate;
}
```

- `secret name;` is a required protected client-builder input.
- `secret name?;` is optional initial protected input; a derived credential slot begins unavailable when absent.
- a custom provider object is required; its acquisition may report unavailable;
- an endpoint-backed source begins empty because of source kind and does not need `?` on the credential declaration;
- credential dependency order is resolved from the complete graph, not source order;
- endpoint paths are client-root-relative semantic paths and may be forward references;
- unknown references and cycles are compile errors.

Optional static secrets are prototype-gated. Keep them unless the client-construction task study shows separate client/profile construction is materially clearer.

## Build the client

Required secrets/providers are direct builder inputs. Optional secret uses exact optional controls appropriate to secret wrappers. Secret-bearing values never implement revealing `Debug` or public `Display`.

A declared auth placement is not optional merely because material may be unavailable. Execution fails before dispatch with a credential-unavailable category rather than silently omitting the placement.

## Attach private placement

```concord-pseudocode
auth {
    header "X-Api-Key" = bootstrap challenge never;
}

auth replace {
    bearer session challenge unauthorized;
}

auth off;
```

- ordinary block appends local ordered placements;
- `replace` discards inherited placements;
- `off` removes inherited placements for the subtree;
- empty blocks are rejected;
- public query/header patches cannot mutate private placements.

Supported roles include bearer/basic/custom `Authorization`, validated private header, and validated private query.

## Exclusive ownership

Before dispatch validate:

- one owner for `Authorization`;
- no duplicate case-insensitive private header target;
- no ambiguous private query target;
- no public/private/media/transport collision;
- at most one effective placement with a non-`never` challenge policy.

No last-writer precedence repairs an authority conflict.

## Challenge is attribution

```concord-pseudocode
challenge never
challenge unauthorized
challenge unauthorized_or_forbidden
```

The placement classifies which response statuses are evidence about one applied credential. It does not promise refresh or replay.

Source reaction:

| Source | Reaction |
|---|---|
| static secret-derived material | reject exact version; require replacement |
| endpoint-backed slot | reject/clear exact version; application reacquires through source endpoint |
| refresh-capable provider | single-flight refresh exact version; install only newer material |
| non-refreshing provider | invalidate/fail according to declared recovery capability |

## Versioned slot state

Conceptually:

```text
Empty
  → Acquiring(generation)
  → Ready(version, material, expiry)
  → Rejected(version)
  → Refreshing(version, generation)
  → Ready(new version) | Empty/Cooldown
```

Core owns:

- generations and versions;
- single-flight acquisition;
- waiter cancellation;
- finite cooldown;
- stale-completion rejection;
- manual replace/clear precedence;
- clone-sharing semantics.

An older acquisition/recovery result cannot overwrite a newer manual or successful replacement.

## Recovery replay

One direct/page operation admits at most one auth-recovery replay:

```text
status matches the one attributed challenge
+ exact applied credential version still owns the rejection
+ source produces usable newer material
+ body is replayable within the page/request
+ deadline remains
+ every rate policy admits another visible attempt
+ no previous auth recovery for this page/direct operation
→ one recovery replay
```

This is separate from application policy retry. It may replay a non-idempotent endpoint only because the challenge declaration asserts the remote operation did not enter the endpoint’s accepted effect path. Generated documentation must make that proof obligation explicit.

## Endpoint-backed acquisition

```concord-pseudocode
credential session = endpoint authenticate;
```

Generated only on the source endpoint:

<!-- concord-source: crates/concord-examples/src/authentication/usage.rs#authenticated_requests -->
```rust
    let _: authentication_api::ApiKeyRequest = client.api_key();
    let _: authentication_api::TokenAuthorizationRequest = client.token_authorization();
    let _: authentication_api::BearerRequest = client.bearer();
    let _: authentication_api::BasicRequest = client.basic();
    let _: authentication_api::OAuthRequest = client.o_auth();
    let _: authentication_api::EndpointBackedRequest = client.endpoint_backed();
    let _: authentication_api::ProviderBackedRequest = client.provider_backed();
    let _acquisition = client.login().store_as_acquired();
```

`store_as_session`:

- uses the same executor and source endpoint contract;
- converts a complete accepted result;
- atomically installs material if no newer version superseded it;
- returns `Result<(), ApiError>`;
- does not return or clone protected material;
- is absent from unrelated endpoints.

Ordinary `.await` returns the endpoint’s domain output and does not secretly store it.

## Credential facade

```text
let slot = client.credentials().session();
slot.is_available();
slot.clear()?;
slot.replace(material)?;
```

No raw read accessor. Client clones share slot state; independent builds have distinct security-context identities.

Runtime endpoints carrying credential references are bound to the originating security context and cannot run under a separate build solely because the generated client type matches.

## Provider isolation

Provider receives only:

- acquisition/recovery reason;
- current material when required by refresh semantics;
- remaining deadline/cancellation;
- isolated provider HTTP with finite limits and allowed origins.

It never receives the generated client, application native request/response, endpoint body/output, limiter, operation observer, application client identity, or generic runtime context.

Proxy/root-trust sharing is explicit. Endpoint credentials and application client identity never propagate.

## Information-flow evidence

Sentinel tests must prove credential material appears only in protected slot/provider/native-auth boundaries—not plans, descriptions, errors, observers, rate keys, semantic diff, tracing, stream errors, or endpoint-backed storage results.
