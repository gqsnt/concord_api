# Review h08-candidate-review-1-sol (CANDIDATE review, H08 B, on 3c5323f6)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T10:45:04Z–10:50:15Z, thread 01a1bac3-03e7-7d71-ab27-0fd434016adf.
**Verdict: ACCEPT_WITH_FIXES** (0 blocking, 2 non-blocking). Per point: (1) macro/core PASS
(parsing, visibility, naming, collisions, cfgs, ALL, conversions, delivery, template and `Sends`
bounds); non-adopter isolation PASS — the reviewer independently compared the raw public JSON
indexes of all 52 non-opting SDKs (docs/links removed): identical incl. ids, spans, signatures;
Nager and non-opting scale expansions byte-identical; no kill condition. (2) witness behaviour
PASS (Strava scope/quota/retry/uncertainty/wording; Sentry buckets/read-write/paging; Kraken nonce,
body signing, sensitive headers, envelope, single dispatch; `MissingCredentials` correct). (3)
Kraken test edits PASS (remove assertions of deliberately changed refusals; coverage kept). (4)
concept budget/A7 PASS (one statement, one core trait; hand-written classification replaced in 3
SDKs; allocations identical; opt-in costs +1.7% check, +1.9% build, +3.7% expansion). (5) docs
PASS except the findings. (6) **B fits the card better than A** (A: 55 added entries, 33 changed
signatures, diagnostic names the shared output type). Not checked: builds/tests rerun, CI, sealed
measurements.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | `from_operation` matches textual identity (kind, dotted name, method): same-named clients in different modules can match; author guide and Kraken `prepare_signed` promise stronger provenance | non-blocking | fix — docs state textual-identity semantics consistently; a same-named-client regression test |
| 2 | Kraken prefix signing: signing the exact outgoing path breaks behind a proxy that strips the prefix (Kraken signs from `/0/private`) | non-blocking | fix — document the limitation in the Kraken README/docs (no behaviour change in this pilot) |
