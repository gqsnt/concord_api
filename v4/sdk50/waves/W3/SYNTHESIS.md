# Wave 3 synthesis (SDK31–35, payments and accounting) and the changes made before Wave 4

Wave 3 froze Concord at `14b9e1e0` and built Adyen Checkout, PayPal, Square Payments (simple
control), QuickBooks Online and Xero Accounting, each with an author, a fresh docs-only consumer
and an independent maintainer (15 roles; routing R-1: Sol and Opus only; ledger in
[`../../MODELS.md`](../../MODELS.md)). Findings: [`FINDINGS.md`](FINDINGS.md).

| SDK | Author | src lines | Consumer | Maintainer ticket (from provider docs) |
|---|---|---:|---|---|
| SDK31 Adyen | Sol | 917 | Opus, docs-only, 0 mismatches | manual capture (`CaptureReceived`, echo check) |
| SDK32 PayPal | Opus | 1,745 | Sol, docs-only, 0 mismatches | authorize → capture / void |
| SDK33 Square (control) | Sol | 354 | Opus, docs-only, 0 mismatches | delayed capture (complete with version token, cancel) |
| SDK34 QuickBooks | Opus | 2,488 | Sol, docs-only, 0 mismatches | invoice payments (exact decimals, `requestid`) |
| SDK35 Xero | Sol | 1,520 | Opus, docs-only, 6 mismatches | incremental sync (`If-Modified-Since`) |

## What Concord learned

**1. Declarations held; the cost moved into policies that cannot see the declaration.** Every
maintainer extended its SDK by adding endpoints to the existing client → scope → endpoint tree
(+18 declaration lines for Square's delayed capture; an optional `Utc` value feeding a header
template for Xero) and no maintainer needed a core change or a native escape. What repeated
instead was a policy or SDK re-deriving a declared fact: a request id parsed back from the URL
(QuickBooks author and maintainer) or header (PayPal author and maintainer), a payment reference
re-read from the path (Adyen maintainer), retry permission kept as an operation allowlist beside
the declaration (PayPal: removing an entry compiled), an operation compared through
`<E as Endpoint<Output>>::operation()` (Square), and handles mirrored because generated types have
setters but no getters (Xero: "this traversal's filter" in nine places). With Wave 1's SDK23/24
this is the strongest core/macro witness so far (B12-M, B23-F3-3).

**2. A method-based "this write may have happened" rule silently grows.** Square's
`create_outcome_unknown()` decided by HTTP method; two new POST endpoints made it true for
completes and cancels with every old test green (maintainer). `delivery()` answers what Concord
can prove; which operation a rule covers needs the operation, not the method.

**3. Idempotent writes are one requirement with four mechanics.** Square: the caller keeps a
clone and replays the identical bytes (body key). Adyen: a 917-line SDK with a bounded replay
ledger (header key, expiry, reservations). PayPal: an allowlist of operations that accept
`PayPal-Request-Id`. QuickBooks: a `requestid` query value on every create. Shared: "this
endpoint may be replayed under a key, with identical bytes, until a deadline". Provider-specific:
where the key lives, how long it holds, what a mismatched replay returns. Concord already supplies
byte-identical replay (`Prepared` cloning, `concord-retry`); what is missing is (1) above —
declaring which endpoints carry the key and letting the policy read it.

**4. Tenants and realms: the hierarchy works, the generated surface leaks.** QuickBooks bound
the realm and its token to one client value that is both the send policy and the path value —
realm and credentials cannot diverge. Xero wrapped the scope because generated setters and
`with_base_url` can change a tenant header; PayPal added an origin check because
`with_base_url` can move a client away from its credentials. The fixed hierarchy expresses
ownership; the generated mutators undo it.

**5. `send` is client-wide.** PayPal needed a second `api!` client for the token endpoint on the
same host (base URL written three times); QuickBooks made the realm a client value because a
scope cannot carry `send` and a policy cannot read scope values.

**6. Capability crates under real pressure.** `concord-retry`: four roles in three SDKs (Adyen,
Xero, PayPal ×2) needed an async or fallible before-attempt hook and abandoned `receive` for
`run` + `into_parts`. `concord-auth`: three rotating-refresh-token providers (HubSpot, Xero,
QuickBooks) each built rotation, seeding and failure memory by hand; Xero's fail-closed journal
erases the old refresh token even when no pair was issued (consumer F11). `concord-pagination`:
adopted where listings exist; a listing-bound continuation (Xero) and a page-number helper are
single witnesses. `concord-ratelimit`: no adoption again (no published quotas).

**7. Docs-only consumers succeeded; the generated docs were the friction.** Five consumers, zero
source lookups, one prediction table with mismatches (Xero's credential semantics). Repeated
friction: generated endpoint docs put content-type mechanics before method/path/query (fixed),
a garbled value-policy sentence (fixed), `SendWith<T>::Output` named when the client declares the
result (fixed), the declared production URL shown unlabelled (fixed), and `--no-deps` rustdoc
shows `concord::Error` as a bare `Error` (three consumers).

**8. Simple control.** Square: 354 source lines; its consumer had no mismatch; its maintainer
added three endpoints with +18 declaration lines.

## Evidence caveats

- 20 of 28 Codex wave roles in Waves 1–3 read the handoff's mission files above their worktree
  (the author capsule said "Read AGENTS.md/CLAUDE.md"); Opus subagents receive the project and
  global CLAUDE.md by harness injection. No role read records, briefs, other roles or the cohort
  file; the mission files name no held-out provider. Consumers were docs-only with respect to the
  SDK but campaign-aware. Capsules now forbid reads outside the worktree.
- Several Opus roles wrote temporary files to the shared session scratchpad (fixed in later
  prompts). Every commit was checked by diff and a coordinator re-run.

## Changes made before Wave 4

| ID | Change | Placement | Witnesses / evidence | Review |
|---|---|---|---|---|
| B34-D | Generated endpoint docs: method/URL, query and headers first; the URL labelled as the declared base; "the client's send policy (the client value `x`)"; with `send P -> Template;` no `SendWith<T>::Output` | MACRO (docs) | SDK33, SDK31, SDK34 consumers; SDK34 author F6 | b34-review: no defect |
| B34-R | `concord-retry::Retry::receive_with`: an async, fallible before-attempt hook owning the request; context `A` reaches classification and is dropped before backoff; `ReceiveReport { completed, refused, no_response }` | EXTERNAL | Adyen F1, Xero F3/F4, PayPal F2/F3 + maintainer | b34-review: no defect |
| B34-O | Inherent `GeneratedEndpoint::operation()` (no output type); `Endpoint<T>::operation()` delegates | MACRO | Square maintainer; PayPal allowlist; kept by both b34-values variants | b34-review2 |
| B34-W | Native Windows: the root `.gitattributes` fix verified (FRED, USGS pass); two refused-connection tests given a 30 s timeout (Windows retries refusals ~2 s) | tests | CI run 42338837366 | b34-review2 |

Adoption: `receive_with` in PayPal (−61/+37 production lines). Adyen not adopted (an existing
assertion pins factory-start accounting; the lane stopped as briefed); Xero not adopted (−5,
needed a manifest edit outside its brief). Square adopts `operation()` (−2/+1).

## Not accepted, and why

| # | Candidate | Disposition | Why |
|---|---|---|---|
| B34-A | Stateful `TokenCache` rotation (`with_state`, `insert`, `replace`, `state`, `RotationError`) | **Not accepted** (prototype `5304d763`) | QuickBooks −10, Xero +4, HubSpot +11: two of three providers grow. Rotation semantics (what a failed refresh consumed, where the pair is durable) stay provider- and application-owned. |
| B34-V | Declared values readable back: A rule-based getters; B `facts { Key = value; }` + `endpoint.fact::<Key>()` | **EXPERIMENT continues** (prototype `4fa26231`; report `docs/experiments/b34-values.md` on that branch) | A −9 SDK lines but cannot serve generic policies and must exclude sensitive values; B +5 SDK lines and ~156 core/macro lines for a new declaration concept, removes PayPal's allowlist but cannot guarantee that a write declares its key, and weakens Xero's required reads to `Option`. Neither pays for itself yet; Wave 4 witnesses decide. |

## Dispositions of the other candidates

| # | Candidate | Disposition | Witnesses |
|---|---|---|---|
| S | `send` on a scope / per endpoint | **EXPERIMENT for W4** | PayPal F1 (second client for the token endpoint), QuickBooks F1 (realm forced to the client); HubSpot's separate OAuth declaration |
| M | Generated mutators undo ownership (setters, `with_base_url` on tenant/credential-bound clients) | **EXPERIMENT for W4** (opt-in restricted view) | Xero F2, PayPal F6/F7 |
| G | Required values in parameter groups | wait | Adyen maintainer |
| I | Initialized constant request bodies | wait | PayPal author F8 + maintainer (one SDK) |
| C | Codec error by value / body kept on a `decode()` failure | wait | QuickBooks F4 |
| P | Listing-bound continuation; checked page-number helper | EXTERNAL, wait | Xero F5, maintainer |
| E | `--no-deps` rustdoc shows `concord::Error` as a bare `Error` | DOCS (SDK re-export guidance) | SDK33, SDK35, SDK31 consumers |
| W | Method-based "write may have happened" rules silently cover new endpoints | DOCS (§5.11b: decide by operation) | Square maintainer |
| RL | `concord-ratelimit` | no investment (0 adoptions in 11 providers) | — |

## Not done, and why

- Live provider calls: NOT_RUN. The refused-connection timeout fix is verified on native Windows
  only by the next CI run.
