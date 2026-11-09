# Concord observations — example-assisted

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

No shared crates were changed. These are bounded observations and proposals, not claims that a
provider rule belongs in Concord. The declaration retains client → scope → endpoint. JSON, form,
path encoding, sensitive headers and generated `send()` cover the ordinary wire work. The SDK
still owns the models and policy below.

1. **Rotating credentials:** `concord-auth`'s single-flight cache publishes an access `Token`
   with header, lifetime and generation, but has no refresh-token replacement transaction, durable
   store, multi-process lock, or way to atomically publish provider-specific state with the snapshot.
   The SDK keeps synchronized installation state and a conservative uncertainty latch. The caller
   owns durable coordination. A small external-auth extension worth investigating is an acquisition
   result with an application-supplied commit hook that can fail before access-token publication;
   durable compare-and-swap and cross-process locking must remain application-controlled. The cache
   alone cannot guarantee safety across separate SDK instances or crashes.
2. **Create uncertainty:** ordinary generated `send()` returns a Concord error but does not know
   whether an endpoint is a non-idempotent write. The SDK maps dispatch/read failure, 408, 5xx, and
   unreadable success to `CreateUncertain`, never retries, and requires reconciliation. The smallest
   generic improvement to investigate is an operation-stage indicator distinguishing pre-dispatch
   construction from transport/body failures; deciding whether a particular write is uncertain
   remains provider/SDK policy. Caller cancellation remains ambiguous.
3. **Status documents and metadata:** default `send()` preserves an unread status response but
   does not decode provider errors. This slice exposes status/head rather than guessing a universal
   provider schema; the caller may inspect the native error response. A bounded provider-error
   adapter around `Received::document` could retain a redacted typed document and its metadata;
   the provider schema, media policy and safety decisions still belong to the SDK. No native
   escape is required by these workflows.
4. **Cursor projection:** `concord-pagination` provides dependent traversal, explicit incomplete
   results and optional caps, but the SDK must map `paging.next.after`, reject an unchanged cursor,
   and retain `total`. The smallest candidate is an optional generic repeated-key detection policy;
   extracting the actual cursor and selecting limits remain provider rules. A cap is required to
   prevent longer cycles; the helper's default is unbounded.
5. **Transport/replay contract:** Concord uses the supplied reqwest client and cannot inspect its
   redirect or retry configuration. The SDK documents the required settings; an opaque client can
   violate the single-send or credential destination guarantees. A caller-visible strict transport
   builder/contract could make that configuration explicit; it should not silently change caller
   TLS, proxy or timeout settings. `concord-retry` was evaluated but is not used: this slice performs
   no retry, especially on create, and its engine cannot decide provider replay permission.
6. **Rate policy:** `concord-ratelimit` is process-local and needs SDK-selected keys/windows and
   observations; those are not supplied by the bounded contract. No admission is guessed or claimed.
   An application can handle 429 and coordinate limits externally. Adding a generic limiter without
   documented provider policy would imply protection it does not supply.
7. **Body sensitivity:** applying `#[sensitive]` to a body produced the compiler diagnostic that
   Concord never renders bodies; the attribute applies only to placed values. Removing it compiled.
   The OAuth form type implements redacted `Debug` independently. Improve the author guide with
   this distinction and an OAuth form example; body values still require care when logging raw
   native requests. `concord-codecs` JSON and Form were used through `concord` features; other
   codecs do not fit this JSON/form slice.

## Example files consulted

- `examples/linear-sdk/src/lib.rs`: generated declaration with `limit`, a pagination adapter,
  response/error patterns and explicit cursor validation. The HubSpot implementation independently
  maps its own cursor and uses ordinary generated sends.
- `examples/linear-sdk/Cargo.toml`: workspace dependency and lint layout.
- `examples/linear-sdk/tests/protocol.rs` and `examples/linear-sdk/tests/workflows.rs`: searched for
  the no-retry HTTP builder setting; no provider fixture expectations were copied.
- `examples/ntfy-sdk/src/lib.rs`, `examples/ntfy-sdk/src/error.rs`: inspected for SDK organization
  and error-handling alternatives; no provider-specific behavior adopted.
- `examples/testserver/src/lib.rs`, `examples/testserver/Cargo.toml`: local fixture API, raw request
  recording, delays, gates and truncated bodies. Testserver is infrastructure, not a provider SDK.
