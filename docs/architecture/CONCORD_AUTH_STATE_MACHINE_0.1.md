# CONCORD_AUTH_STATE_MACHINE_0.1

**Version:** 0.1  
**Status:** Internal Release Approved
**Authority:** Intended credential slot and authentication-coordination state machine  
**Depends on:** Product Contract, Public API Specification, Execution Specification, Security and Limits Specification  
**Supersedes:** None

## 1. Purpose

This document defines the internal state machine used to satisfy:

- `EXEC-AUTH-*`;
- `EXEC-RECOVER-*`;
- `BOUND-AUTH-*`;
- generated credential-handle behavior.

The state machine is private. Public APIs do not expose credential states, versions, provider-operation identifiers, cohorts, or leases. The public observability `OperationId` is unrelated to credential-provider operations.

## 2. Goals

For each credential slot:

1. at most one acquisition or refresh provider operation is active;
2. all callers waiting on that operation receive the same result;
3. provider code runs without a slot lock;
4. cancellation cannot leave the slot permanently in flight;
5. manual set/clear supersedes active work;
6. stale completion cannot overwrite newer state;
7. exact-version rejection cannot clear a newer credential;
8. provider failure cooldown is bounded;
9. state history remains bounded;
10. atomic static input replacement isolates old and new provider configurations;
11. an explicitly challenge-rejected built-in static credential remains rejected until complete source replacement;
12. credential finalization after limiter waiting cannot select cleared, replaced, rejected, or expired material.

## 3. Ownership model

### 3.1 Public handle

Conceptual internal shape:

```rust
pub struct CredentialRef<M> {
    slot_ref: Arc<CredentialSlotRef<M>>,
}

pub struct CredentialHandle<M> {
    credential: CredentialRef<M>,
}
```

### 3.2 Replaceable slot reference

```rust
struct CredentialSlotRef<M> {
    current: RwLock<Arc<CredentialSlot<M>>>,
}
```

The lock is held only to clone or replace an `Arc<CredentialSlot<M>>`.

### 3.3 Slot

```rust
struct CredentialSlot<M> {
    identity: SlotIdentity,
    credential_id: CredentialId,
    provider: Arc<dyn CredentialProvider<Material = M>>,
    recovery_mode: RecoveryMode,
    refresh_skew: Duration,
    failure_cooldown_cap: Duration,
    rejection_disposition: RejectionDisposition,
    state: Mutex<SlotState<M>>,
}
```

Every complete static input replacement creates a new slot with a new `SlotIdentity`.

Conceptual source policy:

```rust
enum RejectionDisposition {
    PersistUntilSourceReplacement,
    ExplicitSetRequired,
    ReacquireLater,
}
```

Built-in static sources use `PersistUntilSourceReplacement`; endpoint-backed sources use `ExplicitSetRequired`; provider-backed sources use `ReacquireLater` except while coordinated exact refresh recovery is active.

## 4. Why slot replacement is separate from state mutation

Static credential replacement may change multiple inputs:

- Basic username and password;
- OAuth client id and client secret;
- built-in credential provider configuration represented by several secret inputs.

Updating fields inside one provider risks partial state and races with in-flight provider work.

The design instead:

1. validates all new inputs;
2. constructs a complete new provider;
3. constructs a complete new empty slot;
4. atomically replaces the current slot `Arc`;
5. returns success.

Provider operations that already cloned the old slot continue against that isolated slot. Future preflight clones the new slot.

Application operations do not gain authority to send merely by cloning the old slot. Post-limiter credential finalization compares the public slot reference and refuses old material when replacement completed before finalization. Old-slot completion cannot modify the new slot because identities and mutex state are physically separate.

## 5. State model

Conceptual state:

```rust
enum SlotState<M> {
    Empty {
        next_version: u64,
    },

    Ready {
        value: Arc<M>,
        version: u64,
        next_version: u64,
    },

    InFlight {
        operation_id: OperationId,
        kind: OperationKind,
        previous: Option<ReadyValue<M>>,
        previous_valid: bool,
        next_version: u64,
        cohort: Arc<OperationCohort<M>>,
    },

    Failed {
        error: Arc<AuthError>,
        retry_at: Instant,
        next_version: u64,
    },

    Rejected {
        rejected_version: u64,
        next_version: u64,
    },
}
```

`next_version` may instead live as a separate field under the same mutex. The exact representation is not important.

### 5.1 `ReadyValue`

```rust
struct ReadyValue<M> {
    value: Arc<M>,
    version: u64,
}
```

### 5.2 `OperationKind`

```rust
enum OperationKind {
    Acquire,
    Refresh,
}
```

## 6. Operation cohort

Every active provider operation has one cohort object.

Conceptual interface:

```rust
struct OperationCohort<M> {
    // one terminal result, many subscribers
}

enum CohortResult<M> {
    Success(ReadyValue<M>),
    Failure(Arc<AuthError>),
    Superseded,
    Canceled,
}
```

Requirements:

- subscribers registered before completion receive the same terminal cohort result;
- completion publication wakes all subscribers;
- publication is idempotent;
- a waiter can be canceled without canceling the leader or other waiters;
- no waiter registration race may miss a terminal result.

A `watch`, `broadcast`, or custom notify/result cell may implement this contract.

## 7. State overview

```text
Empty
  ├─ prepare ───────────────> InFlight(Acquire)
  ├─ manual set ────────────> Ready(N)
  └─ manual clear ──────────> Empty

Ready(N)
  ├─ prepare valid ─────────> Ready(N), return N
  ├─ prepare expiring ──────> InFlight(Refresh, previous N)
  ├─ reject static N ───────> Rejected(N)
  ├─ reject endpoint N ─────> Empty
  ├─ manual set ────────────> Ready(N+1)
  └─ manual clear ──────────> Empty

InFlight
  ├─ provider success ──────> Ready(next N)
  ├─ provider failure/cap ──> Failed
  ├─ provider failure/no cap> Empty
  ├─ leader cancel ─────────> previous Ready or Empty
  ├─ manual set ────────────> Ready(next N), cohort Superseded
  ├─ manual clear ──────────> Empty, cohort Superseded
  └─ reject previous ───────> InFlight(previous_valid = false)

Failed
  ├─ prepare before retry ──> Failed, return stored failure
  ├─ prepare after retry ───> InFlight(Acquire)
  ├─ manual set ────────────> Ready(N)
  └─ manual clear ──────────> Empty

Rejected(N)
  ├─ prepare ───────────────> Rejected(N), return AuthenticationRejected
  └─ complete source replace> new Empty slot/new SlotIdentity
```

A `Rejected` state is used only for a source whose policy is `PersistUntilSourceReplacement`. It retains no material value.

## 8. Credential preflight and finalization

Conceptual pseudocode:

```text
loop:
    slot = slot_ref.current.clone()
    now = clock.now()

    lock slot.state

    match state:
        Ready(value, version):
            if value is valid outside refresh skew:
                return Prepared(value, slot identity, version)

            cohort = begin Refresh(previous=value/version)
            unlock
            become leader

        Empty:
            cohort = begin Acquire(previous=None)
            unlock
            become leader

        Failed(error, retry_at):
            if now < retry_at:
                return error

            cohort = begin Acquire(previous=None)
            unlock
            become leader

        Rejected:
            return AuthenticationRejected

        InFlight(cohort):
            subscribe
            unlock
            result = await cohort

            Success(value): return Prepared(value)
            Failure(error): return error
            Superseded: continue loop
            Canceled: continue loop
```

The current slot is cloned before locking. The resulting prepared value is a preflight candidate, not a send lease. It retains enough opaque identity to be finalized later against the public slot reference.

### 8.1 Finalization algorithm

After limiter authority is granted, the executor calls private finalization with the public slot reference and the preflight candidate.

Conceptual behavior:

```text
finalize(slot_ref, prepared):
    current_slot = clone(slot_ref.current)

    if current_slot.identity != prepared.slot_identity:
        return preflight(current_slot) and finalize again

    lock current_slot.state

    match state:
        Ready(same version, valid now):
            return FinalCredential(value/token)

        Ready(newer version, valid now):
            return FinalCredential(newer value/token)

        Ready(expired or inside final refresh requirement):
            start or join refresh

        InFlight:
            join cohort and reevaluate

        Empty:
            start or join acquire

        Failed:
            return cooldown error or start acquire

        Rejected:
            return AuthenticationRejected
```

The public slot-reference comparison is repeated after any awaited provider operation. A clear or replacement completed before finalization therefore wins. Once a `FinalCredential` is returned, a later clear/replacement may race with the native send and is not a retroactive network revocation mechanism.

Finalized material is short-lived request-local state and is never reused for another Concord attempt without another finalization.

## 9. Leader provider execution

### 9.1 Operation setup

Under lock:

1. allocate `OperationId`;
2. create cohort;
3. store `InFlight` with previous state;
4. create cancellation guard containing slot, operation id, and cohort;
5. unlock.

### 9.2 Provider invocation

The leader calls:

- `provider.acquire` for `Acquire`;
- `provider.refresh(previous.value)` for `Refresh`.

No slot lock is held.

### 9.3 Success commit

Under lock:

1. verify state is `InFlight` with the same operation id;
2. allocate the next version using checked arithmetic;
3. store `Ready(new value, new version)`;
4. disarm cancellation guard;
5. unlock;
6. publish `Success` to the cohort.

If the operation is no longer current, the result is stale and discarded. The cohort should already have been published as `Superseded` or `Canceled` by the transition that replaced it.

### 9.4 Failure commit with cooldown

If the provider failure contains a positive capped cooldown:

1. verify operation identity;
2. compute checked retry instant;
3. store `Failed(error, retry_at)`;
4. disarm guard;
5. unlock;
6. publish the same `Failure(error)` to the cohort.

### 9.5 Failure commit without cooldown

Without cooldown:

1. verify operation identity;
2. store `Empty`;
3. disarm guard;
4. unlock;
5. publish `Failure(error)` to the cohort.

Existing waiters receive the operation failure even though the slot is immediately eligible for a later independent call.

## 10. Cancellation guard

The cancellation guard runs when the leader future is dropped before terminal commit.

It performs a short synchronous transition:

1. lock state;
2. verify the matching operation remains current;
3. if a previous value exists and `previous_valid` is true, restore `Ready(previous)`;
4. otherwise restore `Empty`;
5. unlock;
6. publish `Canceled`;
7. ensure all waiters are notified.

The guard does nothing when:

- operation already committed;
- manual set/clear superseded it;
- slot state moved to another operation;
- the slot object is no longer referenced except by the guard.

The guard must not panic.

## 11. Waiter behavior

A waiter:

- never holds the state lock while awaiting;
- does not own cancellation rollback;
- may be dropped independently;
- receives success/failure from the cohort selected when it subscribed;
- loops only after `Superseded` or `Canceled`.

A waiter receiving operation `Failure` returns that failure even if a later operation has already started or succeeded.

This provides deterministic cohort semantics.

## 12. Manual set

For endpoint-backed credentials:

1. clone current slot;
2. lock state;
3. allocate next version;
4. replace state with `Ready(value, version)`;
5. capture any active cohort;
6. unlock;
7. publish `Superseded` to the old cohort.

A stale provider completion sees that its operation id is no longer current and is discarded.

Manual set performs no provider callback.

## 13. Manual clear

1. clone current slot;
2. lock state;
3. replace state with `Empty`, preserving checked next-version counter;
4. capture any active cohort;
5. unlock;
6. publish `Superseded`.

For a provider-backed credential, a later independent request may acquire again.

For an endpoint-backed provider whose `acquire` reports unavailable, the later request reports authentication unavailable.

## 14. `is_set`

`is_set` is an immediate observation.

It:

- clones current slot;
- locks state briefly;
- returns true only when state is `Ready` and the credential is not already expired at observation time;
- returns false for empty, failed, rejected, in-flight, or expired state;
- does not wait or start provider work.


## 15. Exact-version local rejection

An applied credential token contains slot identity and version. Local rejection first verifies that the token belongs to the exact slot object and performs no provider callback.

### 15.1 Matching ready value

When state is `Ready` with the matching version:

- `PersistUntilSourceReplacement` transitions to `Rejected { rejected_version }`;
- `ExplicitSetRequired` transitions to `Empty`;
- `ReacquireLater` transitions to `Empty` when no coordinated immediate refresh will run.

The rejected/invalid material is dropped after releasing the lock.

### 15.2 Version mismatch

When the ready version differs, no transition occurs. A newer credential remains usable.

### 15.3 In-flight previous match

When state is `InFlight` and its previous version matches, `previous_valid` becomes false. Cancellation cannot restore that previous value.

For a built-in static source, no provider operation is normally in flight after the value is ready; complete source replacement uses physical slot replacement and isolates any old operation.

### 15.4 Other state

No transition occurs.

A persistent `Rejected` state never retains material and never invokes acquisition during preparation.

## 16. Challenge recovery interaction

### 16.1 Built-in static `RecoveryMode::None`

- exact local rejection transitions the matching ready version to `Rejected`;
- no immediate acquisition or refresh starts;
- later preparation returns authentication rejection without provider invocation;
- only complete static source replacement creates a new empty slot and permits use of new inputs.

### 16.2 `InvalidateOnly`

- exact local rejection occurs;
- no immediate protected retry occurs;
- the application must explicitly set or reacquire the credential.

### 16.3 `Refresh`

After recovery hook approval and successful recovery body reconstruction, the executor calls the slot's private `recover_exact(token)` operation.

Under the slot lock, `recover_exact` behaves as follows:

- `Ready` with matching version: create one `InFlight(Refresh)` operation retaining the challenged material as `previous`, set `previous_valid = false`, and become leader;
- `Ready` with newer version: return the newer ready value immediately;
- `InFlight` whose previous version matches: set `previous_valid = false` and join the existing operation cohort;
- another `InFlight`: join it and reevaluate after its cohort result;
- `Empty`: create or join one acquisition operation because no current material remains;
- `Failed`: return the stored failure during cooldown or begin acquisition after cooldown.

The leader for a matching ready value invokes `provider.refresh(previous.value, reason = Rejected)` outside the lock. This preserves the challenged material for provider-specific refresh-token or renewal logic without permitting it to remain usable.

If the recovery hook fails before `recover_exact`, the executor performs exact local rejection and returns the hook error.

### 16.4 Proactive refresh race

When a challenge arrives while proactive refresh of the exact previous version is already active:

- the previous value is marked invalid for rollback;
- the challenge recovery joins the existing refresh cohort;
- leader cancellation restores `Empty`, not the rejected previous value;
- successful refresh may supply the recovery attempt.

### 16.5 Multiple credentials

The normalized effective auth plan contains at most one challenge-enabled placement. One terminal response therefore attributes mutation/recovery to at most one credential.

Other credentials in the complete plan remain untouched. Before a recovery send, all credentials are preflighted and then finalized after the second limiter acquisition so the native request still carries a complete current authentication plan.

## 17. Static slot replacement

Built-in static replacement uses `CredentialSlotRef` swap:

```text
validate all new inputs
construct new provider
construct new empty slot with new SlotIdentity
write-lock slot_ref.current
replace Arc
unlock
return success
```

Failure before the swap leaves the old slot unchanged.

Provider work already attached to the old slot may complete only into that old isolated slot. New preflight uses the new slot. An application request waiting before credential finalization rechecks the public slot reference and cannot send old material after the replacement swap completes.

A request that already crossed credential finalization may race with later replacement; finalization is the documented send-selection boundary.

## 18. Endpoint-backed acquisition

Endpoint-backed acquisition follows ordinary endpoint execution.

After decode success:

1. obtain the current endpoint-backed slot through its handle;
2. perform manual set with the decoded material;
3. return success.

If endpoint execution fails, no slot transition occurs.

If the slot was statically replaced between request creation and successful storage, the acquisition terminal uses the handle's current slot at storage time. Endpoint-backed slots are not normally statically replaced, so this primarily protects generated consistency.

## 19. Failure cooldown

Provider `retry_after` is capped before calculating `retry_at`.

Time arithmetic is checked.

A zero capped duration is treated as no cooldown:

- existing cohort receives failure;
- slot becomes empty;
- later caller may retry.

A failed state stores one safe `Arc<AuthError>` and one retry instant. It does not retain unbounded failure history.

## 20. Version arithmetic

Versions are private opaque equality identities.

The implementation may use `u64` with checked increment or a larger opaque token.

Overflow returns an internal authentication error and fails closed.

Versions are never rendered or exposed publicly.

## 21. Race table

| Race | Required outcome |
|---|---|
| Two missing prepares | One leader; both receive same result |
| Waiter canceled | Leader and other waiters continue |
| Leader canceled | State restored/emptied; waiters wake and reevaluate |
| Manual set during acquire | New value wins; old result discarded; waiters reevaluate to new value |
| Manual clear during acquire | Empty wins; old result discarded |
| Manual set during refresh | New value wins; previous refresh result discarded |
| Exact rejection during refresh | Previous cannot be restored; active refresh may still succeed |
| Old response rejects after newer set | Version mismatch; newer value preserved |
| Static slot replaced during old acquire | Old slot isolated; finalization through the public handle selects the new slot |
| No-cooldown provider failure and later caller | Existing cohort returns failure; later caller may start new operation |
| Static rejection and later caller | Later caller returns authentication rejection; no provider call or send |
| Static source replacement after rejection | New slot is empty and independent; old rejected slot remains isolated |
| Cooldown failure and many callers | All receive stored failure; no provider call before retry instant |
| Clear completes during limiter wait | Finalization observes empty state; no old credential send |
| Source replacement completes during limiter wait | Finalization follows new slot; no old credential send |
| Credential expires during limiter wait | Finalization refreshes/acquires under remaining deadline or fails |
| Clear races after finalization | Finalized send may proceed; later operations observe clear |

## 22. Systematic model checking

The state-transition core is implemented so it can run under Loom, Shuttle, or an equivalent deterministic scheduler without Reqwest or Tokio network dependencies.

The model explores leader/waiter cancellation, manual replacement, clear, exact rejection, stale completion, persistent static rejection, preflight/finalization races, slot-reference replacement, and cohort publication interleavings.

Required model properties:

- at most one active operation per slot;
- no lost wakeup;
- no stale overwrite;
- no restoration of a rejected value;
- no provider call from persistent `Rejected` preparation;
- slot replacement isolates old and new state;
- every cancellation path eventually permits progress.
- clear or replacement before finalization prevents superseded send;
- expiration at finalization cannot return an expired final credential.

## 23. Locking invariants

1. `CredentialSlotRef.current` lock is never held while locking slot state.
2. Slot-state lock is never held across await.
3. Cohort publication occurs after releasing slot-state lock.
4. Provider methods are never called under a Concord credential lock.
5. User hook and limiter code never runs under a credential lock.
6. Lock failure or poisoning returns a safe internal/auth error and does not continue with uncoordinated provider calls.
7. Finalization holds no slot-reference or slot-state lock across provider await or native execution.

## 24. Pseudocode: preflight and finalize

```text
preflight(slot_ref, reason):
    slot = clone(slot_ref.current)

    loop:
        now = clock.now()
        lock slot.state

        if Ready and valid(now, skew):
            token = applied(slot.identity, version)
            value = clone(value)
            unlock
            return value/token

        if Failed and now < retry_at:
            error = clone(error)
            unlock
            return error

        if Rejected:
            unlock
            return AuthenticationRejected

        if InFlight:
            receiver = cohort.subscribe()
            unlock
            match await receiver:
                Success(v) => return v
                Failure(e) => return e
                Superseded | Canceled => continue

        operation = create Acquire or Refresh
        guard = cancellation_guard(operation)
        store InFlight(operation)
        unlock

        result = await provider operation
        commit if current
        publish one cohort result
        return committed result
```

## 25. Pseudocode: reject and recover exact

```text
reject_exact(token):
    slot = token.slot
    lock slot.state

    match state:
        Ready(version == token.version):
            if slot.rejection_disposition == PersistUntilSourceReplacement:
                state = Rejected(version)
            else:
                state = Empty

        InFlight(previous.version == token.version):
            previous_valid = false

        otherwise:
            no change

    unlock
```

```text
recover_exact(token):
    slot = token.slot

    loop:
        lock slot.state

        match state:
            Ready(version == token.version):
                operation = begin Refresh(previous=value/version)
                previous_valid = false
                unlock
                result = await provider.refresh(value, Rejected)
                commit operation result if still current
                return cohort result

            Ready(newer version):
                clone newer value/token
                unlock
                return newer value/token

            InFlight(previous.version == token.version):
                previous_valid = false
                subscribe cohort
                unlock
                await cohort and return/reevaluate

            InFlight(other):
                subscribe cohort
                unlock
                await cohort and reevaluate

            Rejected:
                unlock
                return AuthenticationRejected

            Empty:
                begin or join Acquire
                unlock
                await provider acquisition
                return cohort result

            Failed before retry_at:
                clone error
                unlock
                return error

            Failed after retry_at:
                begin Acquire
                unlock
                await provider acquisition
                return cohort result
```

Local rejection occurs before a rejected value can be selected again. Provider refresh receives the exact challenged material only when that version is still current. A newer ready value is never removed or overwritten by the stale challenge.

## 26. State-machine tests

Required deterministic tests include:

- acquisition single-flight;
- refresh single-flight;
- waiter subscription cannot miss completion;
- same failure for current cohort;
- later retry after no-cooldown failure;
- leader cancellation before provider await;
- leader cancellation during provider HTTP;
- exact rejection during refresh;
- manual set during acquisition;
- manual clear during refresh;
- old exact rejection after new value;
- atomic Basic/OAuth slot replacement;
- stale old-slot completion;
- cooldown cap and expiry;
- version overflow fail closed;
- `is_set` immediate semantics.
- clear during limiter wait prevents stale send;
- slot replacement during limiter wait selects new slot;
- expiration during limiter wait refreshes or fails;
- finalization/clear linearization boundary.

Tests should assert public outcomes and state-machine invariants, not a specific mutex or channel type.
