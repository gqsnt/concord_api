# Concord friction — SDK31

Frozen source: `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`. No shared source was changed.
**Example-assisted** via required guide snippets and `examples/testserver`;
[PROVENANCE.md](PROVENANCE.md) records every consulted file and its use.

## F1 — a replay guard cannot fail from the prepared retry hook

Evidence: `crates/concord-retry/src/concord.rs::Retry::receive` takes a hook returning `()`.
This SDK's `guarantee_expiring_during_backoff_prevents_the_next_wire_attempt` obtains one 503,
waits beyond a shortened key lifetime, then refuses the next dispatch; the server records one
request. A synchronous infallible hook cannot express that refusal. `src/policy.rs` therefore
takes `Prepared::into_parts` and supplies a fallible factory to the shared `Retry::run` engine.
No request is rewritten, and Decoder::execute retains declared handling/identity.

Generic mechanics: fallible admission immediately before every attempt; factory cloning and
attempt accounting. Provider semantics: key retention/revocation and identical-request rules.
Smallest proposed change: an opt-in fallible hook/async factory adapter in concord-retry,
returning local refusal alongside the last unread outcome. Preserve the simple current adapter;
no core DSL retry keyword is needed. Engine reports count factory starts rather than actual
wire dispatches, so the SDK documents a refused attempt's accounting separately.

## F2 — exact caller-retry identity and retention are handwritten

Evidence: `src/policy.rs::{Identity,Entry,Reservation,SendPolicy::reserve}` stores bounded byte
identities and guards in-flight calls. Workflow tests assert identical bytes/headers for automatic
and caller retries; changed body, route, destination, cancellation, capacity, expiry and revocation
have executable counterexamples to treating a Clone or a fresh `send()` as fresh permission.
The ledger takes a short mutex only for local state and retains bindings after cancellation.

Generic mechanics: bounded reservations, equality checks, guards and tombstone retention.
Provider semantics: company scope, same-region destination, conservative seven-day age,
credential/body identity and mapping 703/704. Application: durable storage and coordination
across restarts/processes. Smallest proposed shared change: an optional external guarded replay
ledger over opaque identity/key values with explicit retention/capacity policy. Do not infer
idempotency from POST, Clone, or a header name in core/macros. A provider-neutral durable ledger
is not implemented in this bounded author task; the process-local limitation is explicit.

## F3 — native preparation intentionally bypasses SDK send policy

Evidence: AUTHOR_GUIDE §5.13 and generated `CheckoutV71Payments::prepare` build the declared
request, whereas generated `send` invokes SendWith. Direct Prepared::send would skip the ledger,
destination checks and provider error conversion. The README states this boundary and exposes
ordinary send as the supported payment path. The policy's own decomposition is limited to F1.

Generic mechanics: two terminal paths with different policy coverage. Provider semantics:
bypassing payment replay protection can create duplicates. Smallest change: generated rustdoc
could make the selected policy name and bypass warning prominent on prepare/send_raw. Keep
native preparation available and named distinctly from a decorated SDK request. No wrapper per
endpoint or replacement client/scope/endpoint hierarchy was required.

## F4 — payload redaction and payment outcomes stay in provider models

Evidence: Concord marks the credential and idempotency header sensitive and redacts its own
Prepared/ResponseMeta/Error outputs. It cannot choose how `PaymentResponse`, `Action`, Problem
or a public payload should print. `src/models.rs` and `src/error.rs` supply redacted Debug/Display;
protocol tests inject secret sentinels into bodies, response headers, actions and diagnostics.
The SDK also maps pending/action/refusal/unknown result codes and unreadable success bodies to
honest caller-visible states. HTTP 200 never supplies authorisation by itself.

Generic mechanics: safe wrapper formatting and bounded typed error-document plumbing.
Provider semantics: which values are sensitive, which payment outcomes are final/uncertain,
which errors need reconciliation. Existing Received::document and decode_with_meta removed any
need for a custom JSON codec or a duplicated finite-body reader. Smallest potential improvement:
a documented redacted-model/response wrapper recipe in Concord; no core payment error enum or
model derivation is proposed. Keep provider-state interpretation in the SDK.

## F5 — strict wire replay requires an explicitly configured transport

Evidence: Concord/retry rustdoc says reqwest redirects/retries are outside application-attempt
accounting. The SDK writes `Checkout::sandbox` to disable both, set a timeout, validate its key,
and prevent live destinations through ordinary send. Protocol tests reject live URLs before I/O
and show that a 302 is returned while a second loopback server receives no redirected request.
A caller of generated Checkout::new must preserve these transport requirements.

Generic mechanics: transport construction and separate final-body timeout versus retry-head
budget. Provider semantics: forwarding X-API-Key and replaying a payment must remain controlled.
Smallest proposed change: a documented reusable strict-transport recipe returning reqwest's
builder/client, leaving timeout/redirect choice explicit. Do not silently change user-supplied
clients in core. Tests exercise a stalled final body using a short caller transport timeout;
body failures never restart a payment.

## Capability decisions

All five optional capability READMEs were read before selection.

| Capability | Decision and reason |
| --- | --- |
| concord-retry | Used: one attempt owner, count/deadline bounds, capped exponential backoff, Report and strict Retry-After parsing. F1 explains the factory/native composition. No nested retry loop. |
| concord-codecs | Used through concord's default json feature: ordinary Json request encoding, strict success/error media checks and bounded declared handling. No custom codec or IgnoreContentType adapter. |
| concord-auth | Not added: X-API-Key is a static, sensitive header; no token acquisition, expiry/refresh, Basic or Bearer scheme exists in this slice. Cache/admission would add unused behavior. |
| concord-pagination | Not added: paymentMethods is one response, with no cursor/page protocol; details are a payment continuation, not a page. |
| concord-ratelimit | Not added: no fixed provider quota/bucket contract was established by these sources. Explicit transient permission and shared Retry-After parsing handle bounded reattempts; a synthetic fixed window would not establish Adyen quota compliance. |

## Example files consulted

- `examples/testserver/Cargo.toml`: dependency identity and test runtime support.
- `examples/testserver/src/lib.rs`: Recorded/Reply, gates and bounded observable server lifecycle.
- No existing `examples/*-sdk` source file was directly opened. Required guide excerpts supplied
  the quickstart declaration, native decomposition and frankfurter bounded provider-error pattern;
  exact excerpt origins and other background snippets are recorded in PROVENANCE.md.

## Diagnostics and changes

The initial offline compile succeeded. The first full tests passed with two missing-docs warnings
for test crates. Adding crate-level test documentation removed them; Clippy with `-D warnings`
then passed. Later tests tightened missing/mismatched/repeated idempotency confirmation, retained
deadline head evidence and checked unknown/partial states. No compiler-driven shared API edit,
new dependency, or unimplemented shared change is claimed.

Final age hardening checks wall-clock age as well as monotonic age, refusing wall-clock rollback
rather than extending a guarantee across suspension or clock changes. One local compile attempt
reported E0599 for an unavailable Rust Result::is_err_or method; replacing it with negated
is_ok_and corrected the SDK error. This was a Rust authoring diagnostic, not Concord friction.
The required package gates were rerun after that source change.
