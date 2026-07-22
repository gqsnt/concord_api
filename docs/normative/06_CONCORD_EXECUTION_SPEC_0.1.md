# CONCORD_EXECUTION_SPEC_0.1

**Version:** 0.1  
**Status:** Normative Implementation Candidate
**Authority:** Runtime phases, side effects, attempts, recovery, precedence, and cancellation  
**Requirement prefix:** `EXEC`  
**Depends on:** Specification Index, Product Contract, Public API Specification, Security and Limits Specification  
**Supersedes:** None

## 1. Scope

This document owns the exact runtime behavior of:

- client construction side effects;
- pending request creation;
- public request preparation;
- request-body preparation;
- runtime hooks;
- credential preparation;
- rate-limit acquisition;
- native Reqwest execution;
- response-head observation;
- authentication recovery;
- status classification;
- buffered and streaming response processing;
- pagination;
- provider HTTP;
- completion observation;
- error precedence;
- cancellation.

Exact public type signatures are owned by the Public API Specification. Exact numeric limits are owned by the Security and Limits Specification.

## 2. Execution terminology

### 2.1 User operation

One application-visible endpoint action:

- direct await;
- metadata response;
- dangerous raw response;
- stream creation;
- one pagination page.

### 2.2 Page operation

One user operation executing one page of a pagination collection.

### 2.3 Concord attempt

One Concord-controlled application request submission to the managed application Reqwest client.

An ordinary operation begins at recovery index zero.

An eligible authentication challenge may create one attempt at recovery index one.

### 2.4 Reqwest-internal attempt

A native retry performed inside one Reqwest execution. It is not a Concord attempt.

### 2.5 Provider operation

One credential acquisition or refresh operation.

### 2.6 Provider HTTP operation

One call to the isolated provider Reqwest client.

**EXEC-TERM-001 — Recovery index bound.** A Concord attempt recovery index MUST be either zero or one.

## 3. Client construction

Client construction performs:

1. required generated-input validation;
2. optional/default value resolution;
3. base URL validation;
4. fixed public configuration validation;
5. safe HTTP configuration validation;
6. fixed TLS capability validation;
7. retry-selection resolution;
8. effective dangerous custom retry factory invocation;
9. application Reqwest client construction;
10. provider Reqwest client construction;
11. runtime-limit validation;
12. hook, debug, and limiter resolution;
13. required rate-limit enforcement validation;
14. credential-handle and provider-slot construction;
15. generated client construction.

**EXEC-BUILD-002 — Construction side-effect exclusions.** Client construction MUST NOT:

- execute an endpoint;
- call a credential provider;
- execute provider HTTP;
- invoke request hooks;
- acquire rate-limit authority;
- encode endpoint bodies;
- initialize pagination controllers.

**EXEC-CONSTRUCT-001 — No request side effects.** Client construction MUST perform no application or provider network operation.

**EXEC-CONSTRUCT-002 — Fixed HTTPS preflight.** A fixed HTTPS base without usable TLS capability MUST fail construction.

**EXEC-CONSTRUCT-003 — Effective retry factory only.** A dangerous custom retry factory MUST NOT be invoked when another configuration selection overrides it.

## 4. Pending request creation

Creating a request through a facade or `client.request(endpoint)`:

- creates owned endpoint/request state;
- applies generated defaults;
- stores immutable public-value snapshots;
- stores request-local timeout and debug defaults.

**EXEC-CREATE-002 — Pending request side-effect exclusions.** Pending request creation MUST NOT:

- resolve the final URL;
- encode a body;
- invoke a body factory;
- call hooks;
- call providers;
- acquire limiter state;
- build a native Reqwest request;
- execute HTTP.

**EXEC-CREATE-001 — Side-effect-free request creation.** Creating and configuring a pending request MUST be free of provider, limiter, hook, and network side effects.

## 5. Canonical application pipeline

When an endpoint/page future is first polled, Concord allocates one opaque operation identifier and records the operation start instant. A resolved non-disabled `timeout` creates one operation deadline from that instant.

An initial Concord attempt uses this ordering:

```text
1. Prepare the public request head
2. Compute request-specification metadata
3. Apply known metadata limit checks
4. Run the before-send hook
5. Run request-specification preparation once
6. Validate metadata against the produced managed RequestBody
7. Perform ordinary managed-body materialization for the Concord attempt
8. Preflight credentials
9. Acquire rate-limit authority
10. Finalize current credentials
11. Build the native public Reqwest request and emit request debug
12. Materialize authentication and execute Reqwest
13. Emit response-head debug, run the response-head hook, and observe the limiter
14. Handle authentication challenge
15. Classify terminal status
16. Capture a finite complete terminal body, or perform successful mode known-length preflight
17. Run ResponseBodySpec::validate
18. Acquire the sealed buffered, streaming, or discard mode body
19. Run ResponseBodySpec::from_body
20. Check synchronous extension work against the deadline and fix the application result,
    completion outcome, final status, and elapsed time
21. Await one request/page completion event outside the operation deadline
22. Return the already-fixed result
```

The same operation identifier and deadline remain in force across authentication recovery. A page in a pagination collection receives its own operation identifier and operation deadline plus the aggregate pagination operation identifier as parent. The collection has no separate whole-pagination deadline in 0.1.

A failure stops at its phase. Later phases do not run. Earlier completed side effects are not rolled back unless explicitly stated.

**EXEC-ORDER-001 — Canonical ordering.** Every generated and prepared application endpoint MUST follow this ordering.

**EXEC-DEADLINE-001 — Operation-wide result deadline.** A resolved endpoint timeout MUST bound every result-determining request-specification, hook, credential/provider, limiter, credential-finalization, native HTTP, authentication-recovery, terminal capture, response-mode opening/acquisition, response-specification validation, and conversion phase through result fixation; synchronous extension work MUST be checked immediately after return, while completion observation and later caller-driven byte or record stream polling MUST remain outside that deadline.

**EXEC-DEADLINE-002 — Recovery budget.** Authentication recovery MUST use the remaining original operation-deadline budget and MUST NOT reset the deadline.

**EXEC-DEADLINE-003 — Page-local budget.** Each page operation MUST receive an independent resolved operation deadline; Concord 0.1 MUST NOT imply one aggregate pagination deadline.

**EXEC-OPID-001 — Stable operation correlation.** Every event for one direct endpoint or page operation MUST carry the same `OperationId`, and every page operation MUST carry the aggregate pagination operation identifier as parent.

**EXEC-OPID-002 — Concurrent uniqueness.** Operation identifier allocation MUST prevent two simultaneously live operations from sharing an identifier, including across client runtimes whose events can reach the same hook object.

## 6. Public request-head and body-metadata preparation

### 6.1 Inputs

Public-head preparation receives:

- parsed base URL;
- immutable client values;
- immutable scope values;
- endpoint values;
- endpoint metadata;
- request-local timeout/debug overrides;
- the request specification type and stored input;
- the response specification's default accept value;
- auth requirements;
- rate-limit declarations.

### 6.2 Operations

Concord performs:

1. method resolution;
2. host-label resolution and validation;
3. path-segment resolution and validation;
4. ordered public query resolution;
5. public header resolution and validation;
6. `ResponseBodySpec::accept` default `Accept` insertion only when public headers do not already provide `Accept`;
7. request-specification metadata computation using only `RequestBodyContext`;
8. operation-deadline resolution;
9. rate-limit plan resolution and key validation;
10. auth placement resolution;
11. auth material/placement compatibility validation;
12. challenge-policy resolution and single-automatic-challenger validation;
13. auth/public header collision validation;
14. auth/public query collision validation;
15. structural body-kind, exact-length, and lower-bound metadata limit validation without encoding, preparation, or factory invocation;
16. logical URL construction;
17. safe URL rendering;
18. dynamic TLS capability validation;
19. HTTPS enforcement when the effective authentication plan is non-empty.

### 6.3 Prohibited side effects

This phase does not encode a codec, invoke `RequestBodySpec::prepare`, invoke a body or multipart
factory, poll a request stream, open a file, build a native body, invoke a credential provider,
mutate credential state, invoke hooks, acquire limiter state, construct a native request, or execute
HTTP. Request-specification metadata receives no URL, headers, credentials, authentication, limiter,
native request, Reqwest, or transport state.

### 6.4 Failure

Failure returns a configuration- or request-body-metadata-class error and causes no protected side effect.

Examples include invalid routing, invalid public headers, auth collisions, several challenge-enabled placements, authenticated cleartext HTTP, unavailable TLS, invalid exact-length metadata, and a known lower bound already above the request limit.

**EXEC-HEAD-001 — Protected preflight.** Public validation, body metadata validation, auth collision checks, and dynamic TLS checks MUST complete before hook, body materialization, provider, limiter, or transport activity.

**EXEC-HEAD-002 — Logical URL immutability.** The logical URL created by this phase MUST remain unchanged through both Concord attempts.

**EXEC-HEAD-003 — Protected HTTPS preflight.** A non-empty effective authentication plan MUST fail during public preparation when the logical URL uses HTTP, before hook, body materialization, provider, limiter, or native request activity. The structured result is `ErrorCategory::Configuration` with `ConfigurationErrorKind::InvalidAuthentication`.

**EXEC-BODYMETA-001 — No eager materialization.** `RequestBodySpec::metadata` MUST run during public preparation before `before_send` and MUST NOT encode a codec, invoke `prepare`, invoke a body or multipart factory, poll a stream, open a file, build a native body, or access URL, headers, credentials, authentication, limiter, Reqwest, or transport state.

## 7. Before-send hook

### 7.1 Timing

`before_send` runs after successful public-head and body-metadata preparation and before expensive body materialization.

It runs before credential preparation, limiter acquisition, authentication materialization, and native execution.

### 7.2 Context

The hook receives only the sanitized fields defined by the Public API and Security specifications. `known_body_length` is a trustworthy exact length or lower-bound-derived hint when one is available before materialization; it is not guaranteed to be the final encoded length.

### 7.3 Failure and cancellation

A hook failure returns a hook-class error and prevents body codec/factory invocation, credential work, limiter acquisition, authentication insertion, and native execution.

Dropping the operation while the hook future is pending drops that future. Application-owned hook side effects are not rolled back.

**EXEC-HOOK-001 — Veto before body/provider/quota.** A before-send veto MUST occur before expensive body materialization, provider work, and limiter side effects.

**EXEC-HOOK-002 — No protected inputs.** The hook MUST NOT receive credential values, body bytes, native URL, or native request access.

## 8. Request-body specification and materialization

### 8.1 Ordering

`RequestBodySpec::prepare` runs synchronously exactly once after hook approval for each pending
request execution and produces the one stable managed `RequestBody`. Core then validates the earlier
metadata against that produced value before ordinary attempt-local materialization. Every body phase
completes before credential preparation.

### 8.2 Buffered codec and reusable bytes

A selected request codec runs only through the generic request-specification adapter and is invoked
once by `prepare`. Concord validates the resulting complete bytes against metadata and the request
limit and retains them for possible authentication recovery. Reusable bytes are validated and
retained directly.

### 8.3 Direct stream

A produced direct stream remains one-shot and unpolled until native Reqwest execution. Exact or
lower-bound metadata may already have failed during preflight. Application code cannot label
replayability; core derives it only from the produced `RequestBody` form.

### 8.4 Body and multipart factories

A produced reconstructible body or multipart factory is invoked once for the initial Concord
attempt and once again only for an authentication-recovery attempt. `RequestBodySpec::prepare` is
not rerun for recovery. Reqwest-internal retries reuse the already materialized native request and
do not rerun `prepare` or reinvoke the factory.

Multipart aggregate framing remains subject to the request limit during native body polling.

### 8.5 Failure effects

Metadata/produced-body contradiction, materialization, or known-size failure causes no credential
operation, limiter acquisition, request debug event, authentication materialization, native request,
or send. Contradictions include incompatible `BodyKind`, exact-length or lower-bound mismatch,
conflicting content-type ownership, invalid `Generated` ownership, bodyless/body-present mismatch,
and impossible managed-body state. The already approved hook is not rolled back.

**EXEC-BODY-001 — Body before provider and quota.** Request-specification preparation, metadata consistency validation, and attempt-local managed-body materialization MUST finish before credential, limiter, request debug, authentication, native request, or send activity.

**EXEC-BODY-002 — Codec once.** `RequestBodySpec::prepare` and a buffered request codec adapted through it MUST each be invoked at most once per pending request execution, including authentication recovery and hidden native retries.

**EXEC-BODY-003 — Factory per Concord attempt.** A factory retained inside the produced `RequestBody` MUST be invoked once per Concord attempt, including at most one authentication-recovery attempt, and never for Reqwest-internal attempts; factory invocation MUST NOT rerun request-specification preparation.

**EXEC-BODY-004 — Stream laziness.** A one-shot stream produced by a request specification MUST NOT be polled before native Reqwest execution and MUST NOT be resent for authentication recovery.

**EXEC-BODY-005 — Exact-length enforcement.** Core MUST enforce request-specification exact and lower-bound metadata plus produced-stream exact length, fail with `RequestBody` on contradiction, early EOF, or overflow, and withhold the first frame causing overflow from the managed native-body boundary.

**EXEC-BODY-006 — Metadata before hook.** Core MUST compute request-specification metadata and apply every known limit check requiring no materialization before `before_send`; a hook veto MUST prevent `prepare` and every body factory or stream action.

**EXEC-BODY-007 — Metadata and produced-body contradiction.** After hook approval and exactly one `prepare`, core MUST validate metadata against the produced `RequestBody` and fail closed on incompatible kind, length, content-type ownership, generated ownership, body presence, or impossible managed state before every credential, limiter, debug, authentication, native-construction, or send phase.

## 9. Credential preflight

### 9.1 Ordering and reuse

Credential preflight begins after body materialization and before limiter acquisition. Requirements are processed in effective auth declaration order. Several placements referencing the same credential reuse one attempt-local preflight result.

### 9.2 Complete availability

Every required credential must be available or successfully acquired/refreshed before continuing. An earlier credential successfully acquired before a later credential failure may remain cached.

Preflight retains prepared material plus opaque slot identity and exact-version tokens. It does not authorize later use without final validation.

### 9.3 Single operation and cohorts

For one credential slot, at most one acquisition or refresh provider future is active. Concurrent callers subscribe to that operation. Every caller already waiting receives the same terminal result.

A no-cooldown provider failure makes a later independent caller eligible to start another operation; existing waiters do not immediately form sequential replacement leaders.

### 9.4 Persistent static rejection

When a built-in static API-key, bearer, or Basic credential version was used by an explicitly challenge-enabled placement and has been rejected, later preflight through the same unchanged slot returns `AuthenticationRejected` without invoking its provider and without sending the credential.

Only complete source replacement creates a new slot identity and clears this persistent rejection.

### 9.5 Cooldown, locking, cancellation, and stale safety

Provider `retry_after` is capped as specified by the Security and Limits Specification. No credential-state lock is held across provider code or provider HTTP.

Canceling the provider-operation leader restores an eligible previous credential or empty state, wakes waiters, and permits later progress.

Stale completion or exact-version transitions cannot overwrite manual replacement, overwrite a newer provider result, restore a cleared credential, or affect a replacement slot.

### 9.6 Failure effects

Credential preflight failure causes no limiter acquisition, request debug event, authentication insertion, or native request.

**EXEC-AUTH-001 — Single-flight.** One credential slot MUST have at most one active acquisition or refresh operation.

**EXEC-AUTH-002 — Cohort result.** All callers waiting on one credential operation MUST receive that operation's result.

**EXEC-AUTH-003 — No lock across provider.** Concord MUST hold no credential-state lock across provider code or provider HTTP.

**EXEC-AUTH-004 — Cancellation progress.** Provider-operation cancellation MUST NOT leave the slot permanently in flight.

**EXEC-AUTH-005 — Stale safety.** Stale credential operations MUST NOT alter newer slot state.

**EXEC-AUTH-008 — Explicit static rejection.** A built-in static credential version MUST enter persistent rejected state only when its applied placement was explicitly challenge-enabled, and that unchanged version MUST then fail preflight without provider invocation or native send until complete source replacement.

## 10. Rate-limit acquisition

### 10.1 Timing

Limiter acquisition begins only after all credentials are prepared.

### 10.2 Context

The limiter receives safe request metadata and the canonical rate-limit plan, but no body, headers, credentials, native URL, or Reqwest request.

### 10.3 Built-in reservation

The built-in limiter:

1. prunes expired state;
2. refills relevant quota buckets;
3. computes applicable cooldown and quota waits;
4. sleeps when necessary without holding the state lock;
5. reevaluates after waking;
6. atomically reserves all relevant rule/window costs when available.

It does not consume some rules while still waiting for another rule.

### 10.4 Failure

Limiter failure causes no request debug event, authentication insertion, or native request.

Prepared body and credentials remain available or cached.

### 10.5 Cancellation

Cancellation while waiting causes no native request.

The built-in limiter consumes no quota until atomic reservation succeeds.

A successful reservation is not refunded if cancellation occurs later.

**EXEC-RATE-001 — Per-attempt acquisition.** Limiter acquisition MUST occur once per Concord attempt.

**EXEC-RATE-002 — Atomic multi-rule reservation.** The built-in limiter MUST reserve all effective rule/window costs atomically.

**EXEC-RATE-003 — No quota while waiting.** Cancellation during built-in limiter waiting MUST consume no quota.

**EXEC-RATE-004 — Static lexical generated plan.** A normal generated Concord attempt MUST acquire
the static plan produced from lexical declarations; request values, hosts, methods, and pagination
continuations MUST NOT alter its rules or keys.

**EXEC-RATE-005 — Pagination reuses the lexical plan.** Every page and bounded authentication
recovery attempt for one generated endpoint MUST reuse the same lexical rule IDs and global keys;
`page_index` is request-context metadata only.

## 11. Credential finalization

### 11.1 Linearization point

After limiter authority is acquired and immediately before native request construction, Concord finalizes every prepared credential against the currently selected public handle and slot state.

For each prepared credential, finalization:

1. verifies that the public handle still selects the same slot identity;
2. verifies that the exact prepared version remains ready and is not rejected or cleared;
3. verifies that it remains valid beyond the finalization instant;
4. reuses a newer ready valid version when one replaced the prepared version;
5. joins or starts refresh/acquisition under the remaining operation deadline when required;
6. returns one final material handle and exact-version token for native materialization.

Credential finalization is the linearization point for one native send. If `clear()` or complete source replacement finishes before finalization, the earlier material is not sent. A clear or replacement racing after finalization may not prevent the already-finalized native send.

### 11.2 Failure and quota

Finalization failure prevents native request construction and authentication insertion. A limiter reservation already granted is not refunded.

**EXEC-AUTH-009 — Final credential revalidation.** Every prepared credential MUST be revalidated after limiter acquisition and before native request construction for current slot identity, exact version, rejection, clear state, and expiration.

**EXEC-AUTH-010 — Pre-finalization revocation.** A clear or complete source replacement that completes before credential finalization MUST prevent the superseded credential from being materialized for that operation.

**EXEC-AUTH-011 — Finalization boundary.** Credential finalization MUST define the last Concord-controlled point at which clear, replacement, rejection, or expiration can prevent an unsent credential from being selected for the native request.

**EXEC-AUTH-012 — Conservative reservation.** Failure during final credential validation MUST prevent native send but MAY retain an already granted limiter reservation.

## 12. Native public request construction and debug

### 12.1 Construction

After limiter success and credential finalization, Concord constructs a Reqwest request containing:

- method;
- logical URL;
- public headers;
- current attempt body;
- remaining operation-deadline budget when a deadline is enabled.

Authentication is still absent.

### 12.2 Attempt body

For recovery index zero:

- reusable bytes are cloned;
- one-shot sources are consumed;
- reconstructible factories have produced the initial body.

For recovery index one:

- reusable bytes are cloned again after recovery hook approval;
- reconstructible factories are invoked again after recovery hook approval;
- one-shot bodies are unavailable.

### 12.3 Request debug event

The built-in debug dispatcher emits a request-ready event after limiter success and before auth insertion.

It uses safe URL and sanitized public headers.

A debug panic follows normal Rust panic behavior.

**EXEC-DEBUG-001 — Debug before auth.** Request debug output MUST be emitted before authentication materialization.

## 13. Authentication materialization

Authentication is inserted only into the native request.

It may insert:

- bearer `Authorization`;
- Basic `Authorization`;
- a custom auth header;
- native query authentication.

It must not mutate the stored logical URL, safe URL, public headers, hook contexts, or limiter context.

Materialization failure prevents Reqwest execution but does not roll back limiter reservation or earlier provider work.

**EXEC-AUTH-006 — Final boundary.** Credential material MUST be inserted only at the final native request boundary.

**EXEC-AUTH-013 — Finalized material only.** Authentication materialization MUST use only material returned by the immediately preceding credential-finalization phase.

## 14. Reqwest execution

### 14.1 One call per Concord attempt

One Concord attempt invokes the managed application Reqwest client once.

### 14.2 Hidden retries

Reqwest-internal retries do not repeat:

- public-head preparation;
- body codec;
- body factory;
- before-send hook;
- credential preparation;
- limiter acquisition;
- request debug event;
- pagination preparation.

### 14.3 Request-body errors

A managed request-body producer error becomes a safe request-body error.

If Reqwest also returns a less-specific transport error, the recorded managed body error takes precedence.

### 14.4 Native errors

A terminal native error is immediately reduced to a safe timeout, connect, transport, or request-body error.

No raw `reqwest::Error` is retained in the public error.

### 14.5 Failure effects

A terminal native error causes no response-head event, limiter response observation, auth challenge handling, or response decoding.

One user-operation completion event is emitted unless the operation was canceled or panicked.

**EXEC-NATIVE-001 — One Reqwest call.** One Concord attempt MUST make exactly one application Reqwest execute call.

**EXEC-NATIVE-002 — Hidden retry invisibility.** Reqwest-internal retries MUST remain invisible to Concord lifecycle callbacks and pagination.

## 15. Response-head observation

For every final HTTP response returned by Reqwest, Concord performs:

1. response-head debug event;
2. `on_response_head` hook;
3. limiter response observation.

All three occur before auth challenge handling, status classification, or body polling.

### 15.1 Debug event

Emitted for every final status, including:

- success;
- `401`;
- `403`;
- `429`;
- other non-success statuses;
- responses later rejected by limits or codecs.

### 15.2 Hook

The response-head hook cannot replace or veto the response through a normal return value.

Cancellation while it is pending drops the response and prevents later processing.

### 15.3 Limiter observation

Observation receives only safe head metadata.

Observation cannot:

- change current success or failure;
- request an immediate resend;
- prevent auth handling;
- return an error replacing the current response.

A limiter may record internal observation failure for later acquisition behavior.

The built-in governor's default observer considers only final `429` responses carrying a valid positive `Retry-After`; it stores a capped cooldown for future acquisition and never resends the current response.

**EXEC-RESPHEAD-001 — Uniform head event.** Every final Reqwest response MUST produce one response-head debug event and hook call per Concord attempt.

**EXEC-RESPHEAD-002 — Observation before body.** Debug, hook, and limiter observation MUST finish before response-body polling.

**EXEC-RESPHEAD-003 — Current response immutable.** Limiter observation MUST NOT replace or suppress the current HTTP result.

## 16. Authentication challenge handling

### 16.1 Candidate statuses

Challenge handling applies to `401` and, where explicitly configured, `403`.

### 16.2 Single attributed candidate

The effective authentication plan contains at most one placement whose policy is not `Never`. Concord evaluates challenge handling only for that placement. Every other credential remains unchanged by the response.

A response with no challenge-enabled placement follows ordinary terminal status handling.

### 16.3 First challenge validation

Before mutation, Concord validates whether the attributed credential and request body can support recovery.

### 16.4 Response release

When recovery will occur, Concord drops the challenged response without polling its body. When the challenge is terminal, Concord may capture the final body under the finite terminal status-body contract.

### 16.5 Static credentials

A built-in static credential defaults to `Never` and therefore is not rejected by an ordinary `401` or `403`.

When the application explicitly challenge-enables the static placement, an eligible challenge marks the exact applied slot/version as persistently rejected. No immediate resend occurs, and later preflight through that unchanged slot fails without provider invocation or native send.

### 16.6 Endpoint-backed credentials

They default to `Never`.

When explicitly challenge-enabled, an eligible challenge clears the exact applied value and requires explicit application reacquisition. No immediate resend occurs.

### 16.7 Refreshable credentials

The built-in OAuth2 client-credentials bearer defaults to `Unauthorized` and uses `Refresh`.

After recovery hook approval, Concord performs an exact-version recovery operation. When the challenged material remains current, the provider's `refresh(current, ...)` method receives that material. When a newer ready value already exists, Concord reuses the newer value without refreshing the rejected version.

### 16.8 One-shot request body

A one-shot body makes recovery unavailable. The attributed credential is transitioned according to its source policy, the terminal challenge body may be captured under the bounded status-body rules, and no second attempt occurs.

### 16.9 Recovery pipeline

For an eligible first challenge with a reusable or reconstructible body:

1. drop the challenged response unread;
2. verify attributed credential refresh capability and body replayability;
3. run `before_send` with recovery index one using the same operation identifier and retained body metadata;
4. reconstruct or clone the recovery body;
5. start, join, or reuse the exact-version replacement operation;
6. call `provider.refresh(current, ...)` when the challenged value remains current;
7. preflight the complete authentication plan;
8. acquire limiter authority again;
9. finalize every current credential after the limiter wait;
10. build the native public request with the remaining operation-deadline budget;
11. emit the request debug event;
12. insert finalized replacement/current authentication;
13. execute Reqwest;
14. perform ordinary response-head observation;
15. prohibit further recovery;
16. complete normal terminal status/body processing.

The original operation identifier and deadline are never reset.

### 16.10 Second challenge

An eligible challenge at recovery index one:

- performs no third attempt;
- transitions only the attributed credential according to source policy;
- captures the terminal body under the finite status-body rules when enabled;
- returns authentication rejection.

**EXEC-RECOVER-001 — One recovery.** A user/page operation MUST perform at most one authentication-recovery attempt.

**EXEC-RECOVER-002 — Recoverable challenge body unread.** A challenge response body MUST NOT be polled when a recovery attempt will follow.

**EXEC-RECOVER-003 — Static no resend.** An unchanged explicitly challenge-enabled static credential MUST NOT be automatically resent after rejection.

**EXEC-RECOVER-008 — Explicit static rejected state.** An eligible challenge MUST persist static rejection only for the single explicitly challenge-enabled static placement and its exact slot identity until complete source replacement.

**EXEC-RECOVER-004 — Endpoint-backed explicit reacquisition.** Endpoint-backed credentials MUST NOT be automatically reacquired through their endpoint during recovery.

**EXEC-RECOVER-005 — Complete plan.** A recovery attempt MUST contain a complete currently finalized effective authentication plan.

**EXEC-RECOVER-006 — Recovery repeats limiter.** Authentication recovery MUST acquire rate-limit authority again.

**EXEC-RECOVER-007 — Exact refresh input.** When a challenged refreshable credential remains the exact current value, recovery MUST call `CredentialProvider::refresh` with that material; a newer ready value MUST be preserved and reused instead.

**EXEC-RECOVER-009 — Single challenge attribution.** One terminal response MUST mutate or recover at most the single placement whose effective challenge policy is not `Never`; unrelated applied credentials MUST remain unchanged.

## 17. Terminal status classification and bounded body capture

After challenge handling:

- every `2xx` status is successful;
- every other final status is terminal.

Managed clients do not follow redirects. Every `3xx` status is terminal in Concord 0.1.

A terminal application status retains status, sanitized headers, safe parsed retry metadata, and one structural `StatusBody`. The status remains the primary outcome regardless of body capture success.

### 17.1 Capture disabled

When terminal body capture is disabled, the response is dropped and `StatusBodyState::Disabled` is recorded.

### 17.2 Empty and complete bodies

A known zero-length or immediate EOF body records `Empty`. A complete body within the finite status-body limit records `Complete` and is available only through the explicit opaque accessor.

### 17.3 Known oversized body

When a trusted known content length exceeds the status-body limit, Concord does not poll the body and records `OmittedKnownTooLarge`.

### 17.4 Unknown-length overflow

Concord collects until EOF or until the next data frame would exceed the status-body limit. On overflow it discards all bytes already collected, does not expose a prefix, stops polling, and records `OmittedLimitExceeded`.

### 17.5 Read failure and deadline

A response-body read failure records `UnavailableReadFailure`. Expiration of the operation deadline records `UnavailableDeadline`. Both preserve the terminal HTTP status and terminal outcome category.

A final `429` remains an `HttpStatus` result; `RateLimit` is reserved for client-side limiter acquisition/configuration failures.

**EXEC-STATUS-001 — Bounded complete-only body.** A terminal non-success body MUST be exposed only when complete capture succeeds within the finite status-body limit and remaining operation deadline.

**EXEC-STATUS-002 — All 2xx success.** All `2xx` statuses MUST pass initial success classification, subject to the declared response contract.

**EXEC-STATUS-003 — Server status category.** A final `429` or other non-success server response MUST remain an `HttpStatus` outcome even when limiter observation stores a future cooldown or terminal body capture fails.

**EXEC-STATUS-004 — No prefix exposure.** Terminal body limit overflow MUST discard every collected byte and MUST NOT expose a truncated prefix.

**EXEC-STATUS-005 — Capture failure preserves status.** Terminal body read failure, limit exhaustion, or operation-deadline expiry MUST preserve the known HTTP status and MUST NOT replace it with `ResponseBody` or `Timeout` as the primary category.

**EXEC-STATUS-006 — Terminal challenge capture.** A final authentication challenge MAY use the bounded terminal status-body capture path, while a challenge followed by recovery MUST remain unread.

## 18. Sealed successful response acquisition

After response-head hooks, limiter observation, challenge handling, and successful `2xx`
classification, every response specification follows exactly:

```text
mode-specific known-length preflight
-> ResponseBodySpec::validate
-> core-owned mode acquisition
-> ResponseBodySpec::from_body
-> synchronous extension deadline check
-> result fixation and ordinary completion
```

The public-but-sealed modes are exactly buffered, streaming, and discard. A custom response
specification receives a successful logical `ResponseHead` and the core-acquired mode body, never a
native response or transport. It cannot bypass status handling, change limits, replace completion,
or define a fourth acquisition mode.

**EXEC-RESP-001 — Sealed response-mode acquisition.** After successful status classification, core MUST acquire response bodies only through the exact sealed buffered, streaming, or discard mode, MUST retain native response ownership, and MUST run `from_body` only after the selected mode body is ready.

**EXEC-RESP-002 — Response-specification validation before polling.** Core MUST perform mode-specific known-length preflight before `ResponseBodySpec::validate`, MUST finish validation before the first body poll, and MUST check synchronous validation and conversion work against the operation deadline immediately after return.

### 18.1 Buffered mode

#### 18.1.1 Limit selection

The buffered-response limit applies to:

- every `ResponseBodySpec<Mode = Buffered>`;
- dangerous raw response.

#### 18.1.2 Known length

A known content length above the selected limit fails before body polling.

#### 18.1.3 Contract validation

After known-length validation and before body polling, the selected response specification validates
the successful `ResponseHead`.

A response-contract failure does not poll the body or invoke `from_body`.

#### 18.1.4 Collection

Concord polls body frames, counts post-decompression data bytes, and rejects before passing the first
excess frame to a response specification, decoder, or caller.

Native body failures become safe response-body errors.

#### 18.1.5 Conversion

`ResponseBodySpec::from_body` receives complete bounded `Bytes` only. A generic buffered codec
adapter then invokes `DecodeBody` once on those complete bytes.

A decode failure becomes a safe response-decoding error.

#### 18.1.6 Metadata terminal

A metadata terminal wraps the converted semantic result in `ApiResponse<T>` after successful
conversion. Direct await returns the semantic value.

**EXEC-BUFFER-001 — Known excess preflight.** A known excessive buffered response length MUST fail before response-specification validation, body polling, or `from_body`.

**EXEC-BUFFER-002 — Excess frame withholding.** The first frame causing a buffered response-limit excess MUST NOT be retained or delivered to the response specification, decoder, or caller.

**EXEC-BUFFER-003 — Complete body before decode.** Buffered `ResponseBodySpec::from_body` and any adapted codec decode MUST begin only after complete bounded post-decompression collection.

## 19. Discard response mode

For a `ResponseBodySpec<Mode = Discard>` after successful status classification:

1. perform known-length mode preflight;
2. run response-specification validation;
3. drop the native response without polling its body;
4. call `from_body((), &head)`;
5. emit completion success;
6. return the output directly or through `ApiResponse<T>`.

`NoContent` accepts every successful `2xx` and returns `()`. No strict requirement for `204` or
`205` applies. A custom discard specification may return another safe output but cannot poll the
body.

**EXEC-NOCONTENT-001 — Ignore successful body.** `NoContent` and every discard-mode response specification MUST validate before discard, MUST receive only `()` in `from_body`, and MUST never poll the successful response body; `NoContent` MUST accept every successful `2xx`.

## 20. Streaming response mode

Before returning a streaming output, Concord completes:

1. response-head observation;
2. auth challenge handling;
3. success status classification;
4. known-length limit validation;
5. response-specification validation;
6. construction of one unpolled `ByteStream`;
7. `ResponseBodySpec::from_body` conversion and immediate deadline check.

Unknown body chunks remain unpolled.

The `ByteStream` passed to the response specification and retained by an ordinary streaming output:

- exclusively owns and polls the native body lazily;
- contains no response metadata or media marker;
- counts cumulative post-decompression data bytes;
- rejects before yielding the first excess chunk;
- converts native body failures to safe response-body errors.

The user-operation completion event is emitted with outcome `StreamOpened` when the stream response object is successfully created.

Later chunk errors do not emit another endpoint completion event.

**EXEC-STREAM-001 — Lazy unknown body.** Unknown response chunks MUST remain unpolled through response-specification validation and until the selected streaming output containing the `ByteStream` has been returned.

**EXEC-STREAM-002 — Chunk bound.** `ByteStream` MUST count cumulative post-decompression bytes and fail before retaining or yielding the first chunk that would exceed the finite stream limit.

**EXEC-STREAM-003 — Stream-open outcome.** Successful streaming-specification conversion and return, direct or metadata-wrapped, MUST emit `StreamOpened` and MUST NOT claim that later byte or record stream consumption completed successfully or emit a second completion on later failure.

**EXEC-STREAM-004 — ByteStream ownership and lazy boundary.** Core MUST transfer exactly one private native successful body into one metadata-free, media-free `ByteStream`; dropping it MUST cancel and drop native body ownership, and no response specification may access the native response directly.

**EXEC-DEADLINE-005 — Returned stream boundary.** An endpoint operation deadline MUST govern response validation and synchronous streaming conversion through successful direct or `ApiResponse` output return, but MUST NOT govern later caller-driven polling of the returned `ByteStream` or `RecordStream`.

### 20.1 Integrated record stream execution

For `Records<D>`, response-specification validation calls `D::validate` and conversion calls
`D::start`, both before the first `ByteStream` poll. One decoder and its one positive finite staging
bound belong to each returned `RecordStream<D>`.

Network chunks are appended only when the current staging buffer cannot produce another record.
Decoder calls repeat without a core lock until `decode` returns `None`; then the stream may poll one
more body chunk. A record can span arbitrary chunks and one chunk can contain several records. The
first append that would exceed the independent staging bound is rejected without retaining the
offending bytes, and the first completely framed oversized record is not yielded.

At body EOF, the stream calls `decode_eof` repeatedly as allowed by decoder progress. A decoder may
emit a complete final record without a trailing newline. Residual incomplete framing returns
`IncompleteRecord`. Body read and decode failures after return are `RecordStream` items. Dropping the
record or batch stream drops its decoder, staging buffer, and underlying `ByteStream` immediately;
no post-return event changes the already emitted `StreamOpened` completion.

**EXEC-RECORD-001 — Record stream opening.** `Records<D>` MUST run `D::validate` and construct exactly one decoder with `D::start` before body polling, then return one `RecordStream<D>` over the one unpolled `ByteStream` with `StreamOpened` completion.

**EXEC-RECORD-002 — Chunk-independent record framing.** Record decoding MUST treat transport chunks only as byte delivery, MUST support records split across arbitrary chunks and several records in one chunk, and MUST repeatedly decode available records before polling another chunk.

**EXEC-RECORD-003 — Bounded record staging.** Record-stream staging MUST enforce `D::max_buffered_bytes()` independently of the cumulative byte-stream limit and MUST reject the first excessive append or framed record without retaining offending bytes or yielding the oversized record.

**EXEC-RECORD-004 — Exact record EOF semantics.** Record-stream EOF MUST call `decode_eof`, MAY emit a decoder-permitted complete final record without a terminal newline, and MUST return `IncompleteRecord` for residual incomplete framing rather than silently discarding it.

**EXEC-RECORD-005 — Record-stream drop cancellation.** Dropping `RecordStream<D>` or `RecordBatches<D>` MUST synchronously drop its decoder state, staging bytes, and underlying `ByteStream`; later body or decoder failures MUST remain stream items and MUST emit no second completion event.

## 21. Dangerous raw response

Dangerous raw execution shares the ordinary pipeline through successful status classification.

It then:

1. applies the buffered response limit;
2. collects complete bounded bytes;
3. skips the declared endpoint decoder;
4. returns successful raw metadata and bytes;
5. emits completion success.

It does not expose a native Reqwest response or non-success body.

**EXEC-RAW-001 — Runtime parity.** Dangerous raw response MUST retain ordinary request-specification preparation, auth, limiter, hooks, status, buffered-limit, deadline, cancellation, and completion behavior, MUST use a core-private eligibility capability rather than macro type-name matching, and MUST never expose a native response.

## 22. Completion observation

### 22.1 Direct and page operations

One direct endpoint operation emits at most one terminal `on_complete` event during normal result return. Each page operation independently emits one such event. Concord first fixes the application result, completion outcome, and final status. It then records elapsed time from first poll through that result-fixation instant, constructs the event, and awaits the completion hook outside the operation deadline before returning the already-fixed result.

The completion hook cannot replace or mutate the fixed result, request a retry, or restart the operation deadline. It is not detached and may delay delivery of the fixed result. Its execution time is excluded from `CompletionEvent::elapsed` and is governed by extension-owned boundedness.

### 22.2 Recovery

Authentication recovery does not emit an additional request/page completion event. The final event records whether recovery occurred.

### 22.3 Stream creation

A successfully returned lazy response stream emits `StreamOpened`, not full `Success`. Later chunk failures or caller drop do not emit another core completion callback in 0.1.

### 22.4 Pagination aggregate

Every pagination collection allocates one aggregate operation identifier. Every nonzero collection emits exactly one `on_pagination_complete` event after final success or failure. `Take(0)` also emits one aggregate success event with zero pages and zero items, while invoking no page/controller/request extensions.

The aggregate event is additional to per-page completion events and reports only safe counts and an outcome category. `on_pagination_complete` observes the fixed aggregate result outside any page deadline and follows the same non-detached, result-immutable completion rules.

### 22.5 Coverage

Normal terminal results emit request/page completion for success and every ordinary error category after operation start. A normally polled timeout result invokes `on_complete` with `CompletionOutcome::Timeout`. Cancellation or panic, including cancellation or panic while a completion future is pending, does not guarantee completion delivery.

**EXEC-COMPLETE-001 — One outside-deadline request/page event.** Each direct endpoint or page operation MUST emit at most one completion event during normal result return. Concord MUST await it outside the operation deadline only after fixing the result, and the hook MUST NOT alter that fixed result or request another attempt.

**EXEC-COMPLETE-002 — One aggregate pagination event.** Each pagination collection MUST emit exactly one aggregate pagination completion event during normal result return, including zero-page success.

**EXEC-COMPLETE-003 — Stream-open semantics.** Every streaming-mode endpoint MUST use the `StreamOpened` outcome when returning its lazy `ByteStream`, record stream, custom streaming output, or metadata wrapper and MUST emit no second completion after later polling failure or drop.

**EXEC-COMPLETE-004 — Aggregate outcome parity.** Pagination aggregate completion MUST preserve the returned ordinary page-error category and use `Pagination` only for pagination-specific cycle, controller, cap, or aggregate-ceiling failures.

**EXEC-COMPLETE-005 — Correlated result-fixation metadata.** Request/page and aggregate pagination completion events MUST carry their allocated operation identifiers, the monotonic elapsed duration from first poll through result fixation excluding completion-hook time, and the final application status when one existed.

## 23. Pagination execution

### 23.1 Entry and preflight

`request.paginate(limit).await` consumes the request.

`PageLimit::Take(0)` returns an empty vector without controller construction, endpoint planning, page hooks, credential work, limiter work, or HTTP. It still emits the aggregate zero-page completion event required by `EXEC-COMPLETE-002`.

For any nonzero limit, Concord validates the requested page count against the configured maximum page ceiling before controller construction. Failure emits one aggregate pagination completion event and no page event.

### 23.2 Initial state

Concord then constructs the complete controller exactly once from the original endpoint snapshot and creates:

- one initial controller;
- one empty result vector;
- one empty request-identity set with tracked retained bytes;
- page index zero;
- aggregate page/item/recovery counters.

### 23.3 Per-page preparation

For each page:

1. call `validate` on the current controller;
2. clone the original bodyless endpoint value;
3. copy every current bound controller field into its declared endpoint or scope field on that fresh clone;
4. assign the page index;
5. prepare the public request head and body metadata through the ordinary endpoint path;
6. construct the canonical logical request identity;
7. reject an already-seen identity;
8. reject retention that would exceed the configured identity-storage ceiling;
9. retain the identity;
10. execute and decode the page through the ordinary application pipeline.

Initial construction reads each bound destination once. Evolving state thereafter belongs only to the controller. Unbound controller fields remain stable controller-local configuration, no per-page endpoint clone is copied back into the controller, and original endpoint, scope, facade, and client snapshots remain unchanged.

Loop and identity-cap failures occur before page hooks, providers, limiter acquisition, or native send for that repeated/unretainable page.

### 23.4 Canonical request identity

The identity contains:

1. the generated stable endpoint discriminator;
2. the exact HTTP method token bytes;
3. the canonical serialized logical URL after all generated bound-field writes and ordinary public endpoint shaping;
4. the canonical final public request headers before authentication insertion.

Logical URL serialization uses the selected `url` crate's canonical `Url` serialization after host/path/query construction. Query pair order is preserved. Default ports and percent encoding therefore compare according to that canonical serialization.

Header canonicalization:

1. lowercase each header name as ASCII;
2. group values by lowercase name;
3. sort header names lexicographically by bytes;
4. preserve repeated-value order within each name;
5. compare each value by exact bytes.

Authentication headers and Concord-managed authentication query pairs are excluded because they are not part of public request state.

Hashing may accelerate lookup, but a hash match must be confirmed by full structural equality; a hash collision cannot create a false cycle.

For identity-ceiling accounting, the canonical payload size is the checked sum of:

- UTF-8 bytes of the generated fully qualified endpoint identity;
- exact method-token bytes;
- UTF-8 bytes of the canonical serialized logical URL;
- lowercase header-name bytes for every canonical header entry;
- exact header-value bytes for every canonical header value.

The ceiling measures this canonical payload, not allocator or hash-table overhead. The configured maximum page count separately bounds per-entry structural overhead.

### 23.5 Successful page processing

After successful page decoding:

1. if this is the final soft `Take` page, consume items in iterator order, enforce the configured item ceiling before the first excess append, and return success without calling `next`;
2. otherwise read `short_page_size` from the current controller;
3. consume the current controller through `next(self, &page)` and retain its `Result` without propagating an error yet;
4. consume the page through `IntoIterator` in iterator order;
5. fail before appending any item that would exceed the configured item ceiling;
6. count and append accepted items;
7. when `short_page_size` is `Some(n)` and the decoded item count is less than `n`, stop and ignore the retained candidate result;
8. when count termination does not apply, propagate a retained controller error;
9. stop when the candidate controller is absent;
10. fail if a hard cap is reached while a next controller exists;
11. commit the candidate controller only after accepted item processing, then increment the page index with checked arithmetic.

There is no universal empty-page termination. `Some(1)` supplies empty-page termination; `None` means decoded item count does not independently stop pagination. Controller origin or type name does not alter this ordering.

### 23.6 Failure and completion

Hard-cap, cycle, controller, page-ceiling, item-ceiling, identity-ceiling, or later page failure drops accumulated items and returns no partial vector. No later page is sent.

One aggregate pagination completion event records final success/failure, completed page count, accepted item count, and whether any page recovered authentication.

### 23.7 Concurrency

One pagination collection has at most one active page operation. Separate pagination collections may execute concurrently.

**EXEC-PAGE-001 — Zero page no-op.** `Take(0)` MUST perform no controller construction, validation, advancement, short-page query, binding, endpoint planning, page extension invocation, credential work, limiter work, or HTTP.

**EXEC-PAGE-002 — Sequential pages.** One pagination operation MUST have at most one active page operation.

**EXEC-PAGE-003 — Always-on cycle protection.** Every prepared page request identity MUST be checked against all earlier page identities before send.

**EXEC-PAGE-004 — No repeated send.** A repeated page identity MUST NOT invoke hooks, providers, limiter, or transport for the repeated page.

**EXEC-PAGE-005 — Controller commit after acceptance.** Candidate next controller state MUST be committed only after successful page decode, accepted item processing, item-limit enforcement, natural-stop selection, and retained-error handling.

**EXEC-PAGE-006 — All or error.** Pagination failure MUST return no partial item vector.

**EXEC-PAGE-007 — Same page recovery.** Authentication recovery within a page MUST retain the same page index, bound page identity, and current controller state.

**EXEC-PAGE-009 — Canonical identity.** Page-cycle equality MUST use the exact endpoint, method, logical URL, and public-header canonicalization defined in this section and MUST confirm full equality after any hash match.

**EXEC-PAGE-010 — Page ceiling preflight.** A nonzero requested page limit above the configured runtime ceiling MUST fail before controller construction.

**EXEC-PAGE-011 — Item ceiling.** Pagination MUST fail before appending an item above the configured collected-item ceiling.

**EXEC-PAGE-012 — Identity ceiling.** Pagination MUST fail before retaining an identity above the configured identity-storage ceiling.

**EXEC-PAGE-013 — Deterministic identity accounting.** Identity-ceiling accounting MUST use the canonical component-byte sum defined in this section and MUST use checked arithmetic.

**EXEC-PAGE-015 — Controller-selected natural-stop precedence.** The final soft page MUST NOT invoke `next`; on every other successful page, controller-selected count termination MUST be evaluated after item acceptance and before a retained `next` error, `None`, hard-cap failure, or next-controller commit.

**EXEC-PAGE-016 — Uniform controller authority.** The core loop MUST apply identical construction, validation, binding, advancement, count-termination, error, and commit semantics to every controller without recognizing controller name, crate origin, arithmetic, cursor semantics, or built-in/custom status; decoded item count MUST NOT terminate independently when `short_page_size` is `None`.

**EXEC-PAGE-017 — Per-page controller validation.** For every attempted page, the current controller's synchronous `validate` method MUST run before endpoint cloning, binding, request preparation, cycle preflight, hooks, credentials, limiter work, or transport, and validation failure MUST prevent all later work for that page.

## 24. Provider HTTP execution

Provider HTTP performs:

1. absolute provider URL validation;
2. HTTPS scheme enforcement;
3. provider TLS capability validation;
4. public provider-header validation;
5. one provider request-body factory invocation when selected;
6. mandatory provider request-limit validation and retention of bounded bytes;
7. native provider request construction;
8. one call to the provider Reqwest client;
9. native error reduction;
10. known provider response-length validation;
11. bounded provider response collection;
11. return of status, actual provider headers, and bytes to provider logic.

Provider HTTP does not invoke application:

- hooks;
- debug output;
- limiter;
- pagination;
- codecs;
- dangerous custom retry.

Non-success provider statuses are returned to provider logic.

When provider HTTP is initiated while preparing a protected application operation, its effective timeout is the earlier of the provider request timeout and the remaining parent operation-deadline budget. Standalone provider operations use only their provider request timeout.

**EXEC-PHTTP-001 — Provider isolation.** Provider HTTP MUST be isolated from application lifecycle extensions and policy.

**EXEC-PHTTP-002 — Provider status ownership.** Provider HTTP MUST NOT automatically classify non-success status as an application status error.

**EXEC-PHTTP-003 — Provider body reuse.** Provider request-body construction MUST occur once per provider operation and MUST NOT repeat for Reqwest-internal provider retries.

**EXEC-PHTTP-004 — Parent deadline propagation.** Provider HTTP initiated for an application operation MUST use no more than the remaining parent operation-deadline budget.

**EXEC-PHTTP-005 — HTTPS-only provider request.** Provider HTTP MUST reject a non-HTTPS URL before body-factory invocation or native execution.


## 25. Error precedence

When several failures are possible, the earliest completed failing phase wins.

Precedence follows:

1. public request/body-metadata validation;
2. before-send hook;
3. body materialization;
4. credential preflight;
5. limiter acquisition;
6. credential finalization;
7. auth materialization;
8. native execution/body producer;
9. response-head hook cancellation/panic behavior;
10. auth challenge handling;
11. terminal status and status-body capture state;
12. known successful response-length limit;
13. successful response contract;
14. successful response body read/stream limit;
15. decoder.

Specific rules:

- a managed request-body error recorded during Reqwest polling takes precedence over a less-specific native transport error;
- auth challenge handling occurs before terminal status return;
- terminal status remains primary while bounded body capture may add only structural status-body state;
- response-body failures occur before decoder invocation;
- ordinary page errors retain their ordinary category instead of being wrapped as generic pagination errors;
- controller-selected count termination precedes a retained next-controller error.

**EXEC-PREC-001 — Earliest phase.** The earliest completed failing phase MUST determine the returned error, subject to the explicit managed-body and pagination-natural-stop precedence rules.

## 26. Side-effect matrix

| Failure phase | Body metadata | Hook | Body materialized | Provider | Limiter | Native request | Response observed |
|---|---:|---:|---:|---:|---:|---:|---:|
| Public request/body metadata | Failed | No | No | No | No | No | No |
| Before-send hook | Yes | Failed | No | No | No | No | No |
| Body materialization | Yes | Yes | Partial/failed | No | No | No | No |
| Credential preflight | Yes | Yes | Yes | Possible/failed | No | No | No |
| Limiter acquisition | Yes | Yes | Yes | Yes | Failed/waiting | No | No |
| Credential finalization | Yes | Yes | Yes | Revalidated/failed | Reserved | No | No |
| Auth materialization | Yes | Yes | Yes | Finalized | Reserved | No | No |
| Native request failure | Yes | Yes | Yes | Yes | Reserved | Attempted | No |
| Response-head cancellation | Yes | Yes | Yes | Yes | Reserved/observed as applicable | Completed | Debug/hook partial |
| Terminal status/capture | Yes | Yes | Yes | Finalized | Observed | Completed | Yes |
| Response limit/read | Yes | Yes | Yes | Yes | Observed | Completed | Yes |
| Decode error | Yes | Yes | Yes | Yes | Observed | Completed | Yes |
| Pagination loop/identity cap | Page metadata only | No repeated-page hook | No repeated-page body | No | No | No | No |

No row implies transactional rollback.

## 27. Cancellation matrix

| Cancellation point | Later work prevented | Effects retained |
|---|---|---|
| Public preparation | All protected work | Synchronous application formatting effects already performed |
| Before-send hook | Body materialization/provider/limiter/send | Hook effects already performed |
| Body materialization | Provider/limiter/send | Synchronous codec/factory effects already performed |
| Credential provider | Limiter/send | Completed provider remote effects |
| Limiter wait | Credential finalization/send | Materialized body and credential preflight cache effects |
| Credential finalization | Native construction/send | Reserved local quota and completed provider effects |
| After limiter reservation | Later send when cancellation wins | Reserved local quota |
| Request upload | Later body polling | Bytes already transmitted |
| Response-head hook | Auth/status/body | Completed HTTP response and debug event |
| Buffered response | Later polling/decode | Bytes already received |
| Returned stream | Later chunks | Chunks already delivered |
| Pagination | Later pages and result return | Completed page effects; accumulated items dropped |

A deadline expiry is reported as `Timeout` and prevents later result-determining phases in the same manner as cancellation observed at that point. During normal continued polling, Concord then fixes the timeout result and awaits its completion callback outside the expired operation deadline. Cancellation while that callback is pending may prevent completion delivery.

**EXEC-CANCEL-001 — No rollback promise.** Cancellation MUST prevent later work after it is observed but MUST NOT imply rollback of completed provider, limiter, hook, network, or response effects.

**EXEC-DEADLINE-004 — Remaining-budget enforcement.** Before and after each awaited result-determining extension or transport phase, Concord MUST enforce the remaining operation-deadline budget; synchronous result-determining extension work is checked immediately after it returns but is not preempted. Completion observation is excluded because it begins only after the result and elapsed duration are fixed.

## 28. Attempt-count guarantees

### 28.1 Concord attempts

- ordinary unchallenged operation: one;
- eligible successful auth recovery: two;
- one-shot challenged operation: one;
- second challenge: two, then terminal;
- each pagination page: independently one or two.

### 28.2 Native attempts

Application retry is disabled by default. With retry disabled, one Concord attempt equals one native Reqwest attempt.

When the application explicitly enables Reqwest default or dangerous custom retry, physical native attempts are Reqwest-owned and may exceed Concord attempt count and one Concord limiter reservation may correspond to several physical attempts.

**EXEC-COUNT-001 — Retry-off equality.** With application retry disabled, one Concord attempt MUST perform one native application attempt.

**EXEC-COUNT-002 — Hidden-count non-guarantee.** Concord MUST NOT promise a stable physical attempt count or per-physical-attempt limiter reservation under explicitly enabled Reqwest default or dangerous custom retry.

**EXEC-COUNT-003 — Retry-disabled default.** Application native retry MUST be disabled unless explicitly enabled through DSL or safe HTTP configuration.
