# Concord friction — SDK35

Historical author notes. P4.3 now fixes required handle values and declares checked runtime bases; see the current README and `docs/MIGRATION.md`.

Baseline `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`; author date 2026-11-07. All proposed shared changes below are records only. No shared crate was edited. The SDK is example-assisted; the complete consultation ledger is in [PROVENANCE.md](PROVENANCE.md).

## F1 — durable rotated pairs are outside a header cache

**Evidence:** `concord-auth/README.md` explicitly leaves persistence outside the crate. `Token` holds only header/lifetime; `TokenCache` shares owned acquisition futures and returns a header/generation snapshot. It has no refresh-token payload, durable transaction, CAS/version protocol, process lock, journal, fsync, recovery or remote/local commit concept. Its ordinary fallible acquisition closure **can** await a commit before returning `Token`, so it is usable without changing the crate. No callback is missing from that extension boundary.

**Written here:** `store.rs` owns bounded records, exclusive create-new process locking, the pre-exchange Refreshing marker, whole-pair staging, fsync/rename activation, recovery and tenant/principal checks. `auth.rs` returns Token only after commit and uses the shared cache for cancellation-safe single flight. The publication-failure workflow causes a real failed rename, reads the retained new pair, verifies no Accounting send, recovers and restarts. Separate clients prove only one exchange is admitted. Lost-response and stale-lock cases prove fail-closed behavior.

**Ownership:** pair scope and rotation are provider semantics; filesystem durability/recovery are application storage mechanics placed in this bounded SDK. The remote issuance/local disk gap cannot be removed by a cache API. Before a complete response is saved, recovery can require reauthorization; the old token is removed from the active record before exchange so it cannot silently remain the sole active credential.

**Smallest useful change:** no core/macro change. Add a durable rotating-pair acquisition recipe to concord-auth's README, including the crash boundary and process ownership. An independently reusable credential journal could remove the generic storage code, but should be an optional external capability, not a persistence mandate in TokenCache. A completion hook alone would not supply atomicity.

## F2 — generated mutability does not encode tenant ownership

**Evidence:** the author guide generates setters for required client/scope values and exposes base replacement. A header String and an invoice UUID do not establish their relationship. Publishing the generated scope directly would allow a tenant header setter beside a policy bound to another grant. Header syntax validation cannot establish authorization.

**Written here:** public immutable Xero/Tenant wrappers; tenant-bound InvoiceRef, ContactRef, DraftInvoice and PageCursor; three small public endpoint operation wrappers; a policy header/binding check before acquisition or I/O. Cross-tenant workflow tests reject all these values plus reopening a journal under another tenant/principal. Importing raw IDs remains an explicit application assertion because connections discovery is excluded.

**Ownership:** provider resource ownership belongs in the SDK. Boilerplate required to expose a restricted generated view is macro/usage cost.

**Smallest useful change:** an opt-in generated API view that omits context/base setters could reduce wrapper forwarding. It would not replace domain reference checks or establish an authorized tenant. Keep normal mutable builders for unrestricted SDKs. Removing the wrappers without an equivalent boundary would transfer validation to every caller; this subtraction is rejected by the cross-tenant cases.

## F3 — async auth plus retry/admission needs the factory boundary

**Evidence:** `Retry::receive` has a synchronous request hook. It cannot await TokenCache acquisition or the limiter, nor return a typed acquisition error. The README explicitly routes this combination to `Retry::run`. Core `prepare()?.into_parts()` and `Decoder::execute` preserve the declared decoder and operation identity.

**Written here:** the send policy prepares once, clones finite request bytes for each attempt, awaits auth, obtains admission, injects the sensitive header, executes exactly once and classifies the unread head. Retry owns the two-attempt/ten-second GET budget. Refresh and create have no replay. This is the only native escape: header injection in a generated send policy. No URL rewrite, alternate raw HTTP client path, hand-built HTTP body or replacement decoder is used.

**Ownership:** retry timing, request cloning and lease mechanics are generic; which errors/writes may repeat and which credential belongs to a tenant are SDK semantics.

**Smallest useful change:** an optional async prepared-attempt adapter accepting a fallible closure could remove the repetitive clone/execute wiring while retaining SDK classification. Existing `run` is sufficient; no new DSL syntax is warranted.

## F4 — retry outcome lifetime and concurrency lease lifetime differ

**Evidence:** Retry keeps its last completed outcome during backoff. Returning a `(Received, Lease)` without intervention would retain the lease while waiting. The ratelimit README requires release before backoff. Final bodies still need the concurrency lease through decoding.

**Written here:** the policy releases the lease on an intermediate retryable 429 and keeps it for a final response. Cooldown observations and singleton Retry-After parsing use shared crates; no scheduler, date parser or FIFO queue was rewritten. Headers with absent/malformed hints are surfaced instead of inventing a quota reset. The long-hint test verifies no early repeat and sibling admission refusal.

**Ownership:** resource lifetime composition is generic. Xero's five-call cap, 429 policy and decision to refuse gated work belong here. Daily/app-wide distributed quota prediction is excluded, rather than simulated with an inaccurate fixed window.

**Smallest useful change:** a documented composition showing a detachable lease beside an unread response would remove discovery cost. An optional before-backoff cleanup callback in the external retry adapter could remove this small branch; core response handling need not change.

## F5 — page-count and legacy envelopes require provider continuation logic

**Evidence:** Xero's requests/responses documentation includes modern metadata and describes the older empty-page traversal. `concord-pagination::Pages` deliberately accepts SDK-selected keys and cannot validate tenant, page size, total/count arithmetic or a provider's envelope casing.

**Written here:** validate metadata, checked page increment, an empty-page fallback, tenant/size-bound cursors, and a factory projecting invoices into `Page`. The shared facade supplies bounded traversal, completeness, typed failed page errors and retained prefixes. The test suite exercises modern final-page completion, short legacy pages, a cap/resume, changed cursor context and a typed failed-page prefix.

**Ownership:** provider metadata fields, compatibility and termination belong in the SDK. Checked page-number arithmetic is generic.

**Smallest useful change:** an external checked one-based page-number helper could remove arithmetic, analogous to next_offset; it should not infer empty-page or metadata precedence. No core change. Replacing this with unconditional `len < size` termination would lose records in the synthetic short-page witness.

## F6 — strict format decoding is not invoice success validation

**Evidence:** Json establishes media and Rust shape, not that a 2xx invoice lacks ValidationErrors, that GET returned the requested UUID, or that a create stayed DRAFT. The ordinary JSON body serializer would also use floats if amounts were modeled as f64.

**Written here:** post-decode invoice/envelope/cardinality/ID checks; exact two-decimal Amount serialization using already-locked serde_json RawValue. The policy uses `Received::document` and boxed provider failures so HTTP errors retain status across malformed, truncated, oversized and wrong-media bodies. No custom XML codec or IgnoreContentType adapter is used: JSON opt-in fits the shared codecs unchanged.

**Ownership:** invoice semantics and bounded monetary input are provider models. Generic JSON/form/media/error-body mechanics are already shared.

**Smallest useful change:** no core or macro change for these provider rules. A shared exact-decimal serde helper could remove numeric serialization only. Decoding a typed error through document already removes a formerly common handwritten bounded-read/media-check cost.

## F7 — traversal result naming required a compiler round trip

**Evidence:** the added interrupted-traversal assertion initially used `Incomplete.items`, analogous to `Page.items`; rustc E0609 listed the actual fields `collected` and `error`. The test was corrected to `collected`, without changing the public crate.

**Written here:** README usage names `Incomplete { collected, error }`; no runtime adapter was needed. A separate Clippy large-enum warning in the SDK's Acquisition was resolved by boxing its Ticket; that was ordinary SDK implementation work, not a Concord limitation.

**Ownership:** public report discoverability belongs to the external pagination API/docs.

**Smallest useful change:** show the exact Incomplete fields beside collect_complete in its introductory example. A breaking rename is unnecessary.

## Shared capability decisions

| Capability | Decision after README/rustdoc reading |
|---|---|
| Concord declaration/core/macros | Used for service context, tenant header, exact route/query/body shape, native requests, identity, limits and safe diagnostics. |
| concord-codecs through facade json/form | Used unchanged; Json's Accept opts into JSON and strict media checks, Form handles OAuth. No XML or streaming feature needed. |
| concord-auth | Used for sensitive Basic/Bearer, bounded tenant/principal/destination-specific cache and generation invalidation; durable journal stays outside it as F1 explains. |
| concord-pagination | Used Pages/max_pages/items/collect_complete with owned tenant-bound factories; metadata interpretation stays in SDK. |
| concord-retry | Used run/Policy/Replay/Decision and strict shared Retry-After; receive's sync hook does not fit async acquisition/admission. |
| concord-ratelimit | Used TokioClock, five-call leases, refusal admission and observed cooldown shared by clones; no invented local model of Xero's daily quota. |

## Example consultation

Directly read `examples/testserver/src/lib.rs` (fixture APIs, gates, body failure and cleanup) and `examples/testserver/Cargo.toml` (dependency identity/features). No existing SDK source file was opened directly.

Read embedded guide excerpts attributed to quickstart `src/lib.rs`/`src/main.rs` (declaration/local construction), shapes `tests/wire.rs` (not adopted), FRED `src/lib.rs` (transport controls), Frankfurter `src/lib.rs` (generic policy/bounded error document), native `src/native_edit.rs` (native boundary), compat `src/lib.rs` (not adopted), OpenAI `src/lib.rs` and Drive `src/upload.rs` (multipart, not adopted), plus the described native `src/repr.rs` codec pattern (not adopted). Exact paths and assistance are enumerated in PROVENANCE. These embedded excerpts make the implementation example-assisted even though no existing SDK was copied as a starting point.
