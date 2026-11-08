# Review h03-decision-review-sol (DECISION review, H03; proposed REJECT before the measurement is read)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T16:13:53Z–16:18:04Z, thread 01a1bbf0-0fdc-7b92-8388-f5f53fa7aa40.
**Verdict: ACCEPT** of the proposed decision. **DECISION: REJECT (LOSS)**, both alternatives,
decidable without the corpus. **Deciding item:** H03's frozen compatibility kill condition fires —
A and B reject baseline-valid borrowed alias inputs and unchanged `V: IntoValue<u16>` forwarders into
Nager's now-exact `u16` parameter. **H03b: LEGITIMATE_WITH_CHANGES** — a separately pre-registered
compatibility trade-off is legitimate; it cannot revise H03's LOSS.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | `examples/renamed/src/lib.rs:30`: `accept: Native = &ACCEPT` (Native = HeaderValue) compiled via `IntoValue<Native>`; A/B need `ACCEPT.clone()` | blocking (for H03) | recorded — part of the deciding item |
| 2 | `native_headers.rs:155,292`: `type Hv = HeaderValue`; initializer `&ACCEPT_JSON` and `.set_accept(&ACCEPT_JSON)` now need `.clone()` | blocking (for H03) | recorded — deciding item |
| 3 | `formats.rs:372-373`: `json_body("x")`, `text_body("x")` with `Raw = String` now need `.to_owned()` | blocking (for H03) | recorded — deciding item |
| 4 | Exact parameters necessarily lose unchanged generic forwarding (`fn forward<V: IntoValue<u16>>`), also opaque `impl IntoValue<u16>`; no exact design avoids it | blocking (for H03) | recorded — deciding item; migration `IntoValue::into_value(v)` noted for H03b |
| 5 | The WIN clause's alias reporting is no exception to the kill condition; "Reject: any kill condition" decides; A1 §3 permits a pilot LOSS | non-blocking | applied — LOSS recorded without waiting for measurement or CI; card and branches kept |
| 6 | The other blockers (B test feature gate, B E0643, raw identifiers) are repairable but are failures on the submitted commits (H03 lists gate failure as LOSS) | non-blocking | recorded in the decision row as additional failures, not the deciding item |
| 7 | H03b K1–K3 incomplete (renamed imports, qualified/generic aliases, associated projections, type macros; K2 must be defined by an input becoming exact for any T incl. opaque values, Option/Cow forwarders) | blocking (H03b draft) | fixed in `cards/H03b.md` |
| 8 | Migration guidance: `IntoValue::into_value(v)` keeps an adapter's signature; function values need wrappers; group traits keep `impl IntoValue<T>` for converting setters only; B′'s narrowing disclosed, its diagnostic requirement unchanged | blocking (H03b draft) | fixed |
| 9 | KEEP_CURRENT clause omitted "or comes with regressions the alternatives cannot avoid"; K1–K4 must not waive corpus regressions, controls, semantics, A5/A6; A8 not blanket authority | blocking (H03b draft) | fixed (clause restored with its scope; waiver limits stated) |
| 10 | Concept accounting per arm (lexical exception; B′ naming rules) | blocking (H03b draft) | fixed (A′ net 0; B′ net +1, disclosed as a prospective change) |
| 11 | Reusing the unseen, hash-frozen corpus keeps blindness but is reused evidence; disclose existing prototypes; record corpus identity and chronology; keep H03's LOSS and INPUT-family accounting; no owner parking needed | non-blocking | applied in `cards/H03b.md` |
