# Concord friction — SDK29 Intercom

Baseline: `8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. Shared code stayed frozen.
Evidence is in this package's source and fixture tests. No speculative problem
below is presented as a failing Concord test. This is **example-assisted**;
the complete per-file ledger and what was taken are in [PROVENANCE.md](PROVENANCE.md).

| Item | Observable cost and evidence | Handwritten responsibility / ownership | Smallest prospective Concord change |
| --- | --- | --- | --- |
| F1: typed errors with metadata need a policy | `src/lib.rs::ResponsePolicy` prepares, receives, branches on status, reads a bounded document and wraps metadata. `protocol.rs` verifies the same status survives each body failure. The generated declaration otherwise handles all three endpoints without wrappers | Intercom's envelope and error meaning are SDK semantics; generic branching/result packaging is repeated mechanics. Boxed provider errors avoid large Result payloads | Optional external `interpret` combinator taking a success projection and bounded error format; retain Received/document underneath. Current ~20-line policy is already a usable boundary, so no new DSL keyword is justified here |
| F2: rate observations have no shared unsigned-header projection | `src/error.rs::number` checks every field line, decimal syntax and overflow. `rate_headers_distinguish_unknown_invalid_and_real_unix_reset` checks missing, duplicate, negative, fractional, list and overflow values | Header names and UNIX reset meaning stay SDK-owned; singleton unsigned parsing and Missing/Invalid distinction are generic mechanics | Add an optional singleton unsigned parser to concord-ratelimit, independent of its scheduler; do not force an absent observation into Duration::ZERO |
| F3: rate scheduler cannot supply unknown provider reset | Read concord-ratelimit README: its local windows/cooldowns require configured limits or actual observations. The required headerless 429 has neither reset nor evidence for a wait. Workflow test returns typed 429 after two page requests, retaining one contact and the failed key | No scheduler was added. Returning unknown is provider semantics and caller decision; a local configured window would be an application assumption | No core change required. A documented external example combining an unknown-429 stop with optional known observations would clarify adoption; a scheduler must never manufacture provider reset evidence |
| F4: retry engine is broader than this slice's replay contract | Read concord-retry README/rustdoc. Its strict Retry-After parser is reused; its attempt engine and optional Concord/Tokio features are not enabled. Tests observe exactly one request for 429/503/disconnected creates | Intercom create ambiguity and stopping GETs are explicit SDK policy. There is no local backoff loop to replace; error inspection does not grant replay | No engine change required. Retain explicit Replay permission; a concise parser-only adoption example would make this choice discoverable |
| F5: cursor adapter and context remain SDK work | `src/pages.rs` maps nested cursor/data into shared Page, passes metadata, rejects empty/immediately repeated cursors and requires a cap. Tests cover empty intermediate pages, explicit resume, cap and malformed metadata | Nested Intercom fields and cursor validity are provider semantics. Context binding to base/auth/version/page size and longer-cycle guarding are generic but currently caller-owned; mandatory cap bounds longer cycles | Optional context-bearing continuation and cycle guard in concord-pagination would remove those remaining obligations without changing client/scope/endpoint. An adapter must still select Intercom's cursor |
| F6: redacting typed provider data | Models and errors derive `concord::Debug` with `#[sensitive]`; Response<T> exposes safe metadata. Canaries cover IDs, cursors, provider fields and headers | Secrecy is declared on SDK models; raw access remains deliberate | Addressed by `concord::Debug` (P4.1). Native and explicit access remain unredacted |
| F7: test crates inherit missing-docs lint | First test run passed 18 tests but warned that protocol/workflows lacked crate docs. Added crate-level docs; this was SDK test scaffolding, not a runtime defect | Two documentation lines, fully owned by this SDK | No Concord change; keep meaningful lint and document test crates |

## Capability decisions and ordinary path

The declaration carries the shared Bearer HeaderValue, fixed version header,
JSON format, success body limit, contact scope and all three endpoints. The
generated `send()` selects the SDK policy; that policy uses `Endpoint::prepare`,
`Prepared::receive`, `Received::document` and `decode_with_meta`. No native
request mutation, send_raw, transport delegation to a vendor SDK or custom
codec was needed. A raw request is exposed only through Concord's existing
explicit prepare API and tests inspecting it.

`concord-auth::bearer` fits static token validation and sensitive header
construction. Its token cache/acquisition machinery does not fit this slice:
the caller supplies a static token and OAuth refresh is excluded.
`concord-pagination::Pages` owns bounded sequential work, typed failure access,
prefix preservation and explicit exhaustion. `concord-codecs` JSON is reused
through `concord`'s default `json` feature; CSV/JSONL/SSE/form/multipart are not
needed. `concord-retry::http` supplies header parsing without scheduling.
`concord-ratelimit` was evaluated and deliberately not added as a dependency
for the reasons in F2/F3. The SDK writes no replacement limiter/retry engine.

Example files consulted: directly, only `examples/testserver/{Cargo.toml,src/lib.rs}`.
Through the required guides' embedded excerpts: quickstart `src/{lib,main}.rs`,
shapes `tests/wire.rs`, fred-sdk `src/lib.rs` (attributed transport recipe),
openai-sdk `src/lib.rs`, google-drive-sdk `src/upload.rs`, frankfurter-sdk
`src/lib.rs`, native `src/native_edit.rs`, compat `src/lib.rs`. The provenance
ledger distinguishes patterns actually used from excerpts considered but unused.
