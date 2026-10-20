# Concord V3 R7 finding dispositions

All eight findings are **CONFIRMED_FIXED** on their demonstrated native paths.
R7-02's simple direct-parent constant hypothesis was rejected by the R6 compiler;
a qualified trait path bypassed that check and natively selected `count=9` instead
of `count=7`. The fix also enables directly authored lexical paths safely.

Baseline: `16f61c0753b313b8c559a4a57dd3dfbe1d2e7345`, tree
`0b1688a9f1e22898328d7d28eeedf87d7d44e5e1`. Every R6 production byte matches the
archive manifest. BEFORE probes use only test overlays in an independent worktree
and target directory. AFTER probes use the correction worktree. Native runs use
Windows x86_64 MSVC, Rust 1.97.1. Fixtures that initially failed to compile are
retained separately and are not defect evidence. The external Edition 1 document
was not supplied; full external-spec conformance was not independently established.

| Finding | Native BEFORE | Correction and native AFTER |
|---|---|---|
| R7-01 | `cargo test -p concord-core --lib --all-features --locked r7_short`: 30s provider made 4 calls, 3 admissions, 0 handoffs and failed. TLS generated endpoint storage succeeded, but its protected request failed preflight. A 120s composed control passed. | Prepared-version proof survives admission restarts, with actual expiry/rejection/replacement rechecked; an active proactive refresh is joined. No-provider installed material remains usable until actual expiry. Runtime and TLS regressions cover 30/60/120s, installation, replacement/expiry during admission and followers. |
| R7-02 | `cargo test -p concord --test r7_qualified_paths --all-features --locked`: compiled and observed `?count=9` instead of `?count=7`. Direct `super::` was a coded compile rejection, not silent rebinding. | Syntax-aware relocation of explicit lexical paths at every emitted Rust use, including contract metadata's representation generic calls; authored export text is retained. Native constructor/output identity, nested scopes/types, qualified adapters, constants, provider types and pagination initializers pass. Bare generated-name collisions remain rejected. |
| R7-03 | `cargo test -p concord-core --lib --all-features --locked r7_decoder`: record and EOF-record escaped, EOF-none succeeded, and consuming-none ran a second callback after expiry. | Same stream precheck after every decoder callback, before outcome selection. All four boundaries and callback cancellation pass; staged/native state and withheld records are released. Existing idle and discarded-next-future controls remain in the gate. |
| R7-04 | `cargo test -p concord --test r7_cooldown --all-features --locked`: actual endpoint without rate declarations immediately dispatched after its 429. | Opaque process-local cooldown origin is independent of quota dimensions; no raw host text is retained or exposed. Actual endpoints verify shared/separate objects, independent/sensitive origins, expiry and zero/malformed delays. Public context literals gain `cooldown_origin: None`; maintained consumers are migrated. |
| R7-05 | `cargo test -p concord-extras --lib --all-features --locked r7_public`: capacity 1 lost B's feedback and admitted B immediately. | One finite object-wide barrier retains the maximum affected expiration when keyed feedback cannot fit. Public observe/acquire tests verify recovery, extension, multiple keys, concurrent cancellation, bounded state and zero partial debit. Existing private capacity reporting remains intact. |
| R7-06 | `cargo test -p concord --test r7_ndjson --all-features --locked`: encoder sent exactly 1 MiB JSON plus LF; buffered conversion and stream framing rejected it. | JSON payload ceiling stays 1,048,576 bytes; bounded staging and default runtime full-frame cap are 1,048,578 bytes for CRLF. Runtime caps count delimiters. Exact/excess, LF/CRLF/EOF, one large/split/one-byte chunk fixtures, blank lines, malformed tails, body/record caps pass. CSV's own framing ceiling is retained. |
| R7-07 | `cargo test -p concord-inspect --lib --all-features --locked r7_duplicate`: a changed duplicate owner was silently structurally equal. | Validate effective indexed keys before lossy indexing: top-level/composite/nested identities, endpoint IDs and positional fallback collisions. Scalar reference arrays retain positional semantics. Native identical/changed/order variants, safe diagnostics and CLI check rejection pass. This is uniqueness validation, not full schema validation. |
| R7-08 | `cargo test -p concord --test r7_oauth_delay --all-features --locked`: real TLS 429 with Retry-After caused 2 immediate helper exchanges. | Core parses the helper delay once at head arrival; remaining time is capped and anchored before body consumption. OAuth errors retain positive rejection hints for the existing slot cooldown. Real TLS 429/503 recovery and native delta/date/zero/malformed/duplicate/overlarge/anchoring controls pass. No new retry loop. |
| R7-S01 | R6 creates stream-only handoff/correlation/cancellation state for all selected drivers. | **CONFIRMED_FIXED** for the narrow allocation obligation: selected-driver gating removes one allocation and 24 bytes per measured buffered execution, warm and cold. Boxed future remains 5,888 bytes; streaming/accepted-bytes/wrapper controls pass. No latency or general V3-O07 acceptance claim. Paired measurements are recorded in `r7_validation.md`. |

Native probes found an adjacent Windows fixture defect: accepted sockets inherited
nonblocking mode and chunked response writes truncated with `WouldBlock`. The
synchronous loopback worker now explicitly selects blocking mode with a finite
write timeout. This fixture repair is distinct from the NDJSON decoder defect;
complete-body baseline probes establish the latter independently.

The earlier selector test expecting two calls for a valid 30s provider result was
updated to one, as required by the composed regression. The awaited-refresh race
now uses genuinely expired intervening material; the already-running proactive
refresh race remains unchanged and passes. A previous private capacity test now
expects the finite barrier and natural recovery. These are intentional oracle
changes supported by the BEFORE failures, not blanket snapshot regeneration.

Provider HTTP-date parsing uses the same configured private wall clock as the
application executor; a frozen-clock native probe proves one head-time read and
monotone remaining delay. No ambient wall clock is added outside its owner.

The native 500-endpoint debug scale fixture also exposed a generated-contract
initialization stack overflow on Windows. Sequential builder assignments alone
still overflowed; endpoint construction now runs in separate private function
frames, preserving contract order and exported text. Its source-specific result is retained separately
from the optional execution allocation comparison.

Pinned 1.97.1 UI expectations were refreshed only after reviewing each mismatch:
14 facade diagnostics retained their rejection obligations, with compiler wording,
trait help/source excerpts and a more precise absence output-type span; two core
diagnostics add standard-library excerpts or list the new opaque cooldown field.
No invalid fixture was converted to a pass fixture and no diagnostic test removed.

Fresh dependency policy found RUSTSEC-2026-0285 in the imported locked Rustls
0.23.42. The lockfile moves only Rustls to patched 0.23.45 and its WebPKI dependency
to 0.103.15. No advisory ignore or policy relaxation was added. Platform gates
started before this discovery are interrupted historical attempts; final
qualification uses the repaired locked dependency set.
