# Review triage (refinement campaign)

Raw reports are archived unedited in `reviews/codex/`. Dispositions: CONFIRMED, FIXED,
REJECTED_WITH_EVIDENCE, DESIGN_TRADEOFF, OUT_OF_SCOPE, BLOCKED.

## Codex design challenge (passes 1–2)

- Reviewer: Codex CLI 0.166.0, `gpt-6.1-sol`, effort `xhigh` — observed in the session logs'
  `turn_context` for every turn. Pass 1 blind (read-only sandbox, isolated directory, only the
  charter, task corpus and contract cases; its command log shows exactly those three reads). Pass 2
  on snapshot `fef06c6` of `8f7adc2`, workspace-write + loopback, private `CARGO_HOME`; interrupted
  by a host restart and resumed on the same session id (source verified unchanged).
- Reports: `design_pass1_blind_20261101T232455Z.md` (sha256 `88e07cd5…`),
  `design_pass2_8f7adc2_20261101T233736Z.md` (sha256 `3c409c07…`).
- Dispositions: every decision-board row is answered in `DECISIONS.md`. Pass 2's confirmed defect
  (header-map capacity panic) → FIXED `ab0278c`; L1/L2 agreement → FIXED `650a174`, `4f3ff85`;
  D01 named roots → FIXED (adopted for ≥ 1 required value rather than ≥ 2, see D01), D13
  `Decoder::finish` → FIXED `f29e9ba`, D14 `cfg`/`deprecated` → FIXED `0864d38`, D18 docs → M6.
  Pass-1 proposals pass 2 itself withdrew or narrowed (retained-baseline reset, scoped
  representation defaults, mandatory adapters, value-identity override subsystem, shared
  retention of every payload) → REJECTED_WITH_EVIDENCE in `DECISIONS.md`.

## Codex milestone review of `6604a53`

- Reviewer: `gpt-6.1-sol`, effort `xhigh` (session `turn_context`), execute mode: workspace-write in
  the disposable snapshot `a3b5407` with network enabled for loopback, private `CARGO_HOME` inside
  the snapshot, fresh `target/review-build`. 2026-11-02 00:28–00:40 UTC. Source integrity after the
  run: all 229 origin files unchanged (`milestone_6604a53_receipt.json`).
- Report: `milestone_6604a53_report.md` (sha256 `44982c5e…`), summary message
  `milestone_6604a53_final_message.md`, selected evidence in `milestone_6604a53_evidence/`.
- Personally executed by the reviewer: fresh locked build; 102 runtime tests on 1.97.1 and on
  MSRV 1.85.1; 36 downstream tests; 8 independent downstream probes (renamed dependency, no
  default features) including an occurrence-table oracle with 1,458 preparations; isolated
  JSON-only and no-default consumers with feature trees; 3 mutation checks (all killed, restored);
  real loopback multipart upload parsed by an independent MIME parser; package listings.
  Verdict: REVISE for F-01–F-04; no high-severity core issue.

| ID | Severity / impact | Evidence | Disposition | Fix / test | Re-review |
|---|---|---|---|---|---|
| F-01 | P2 — native multipart recipe: unescaped `\` in quoted parameters renames fields/files at the recipient | reviewer's MIME parse; reproduced (test fails without the fix) | FIXED | `e05d2f1`; `examples/native/tests/bodies.rs::multipart_parameters_survive_quoted_string_parsing` (independent quoted-string parser) | final review |
| F-02 | P3 — `client r#type` generated a keyword type name | reviewer's `raw-client.log` | FIXED | `e05d2f1`; `tests/hygiene.rs::raw_root` | final review |
| F-03 | P3 — strict rustdoc fails without default features (`Json` links) | reviewer's `rustdoc-no-default.log` | CONFIRMED → docs branch | M6 docs branch; `verify.sh` gains a no-default strict doc step | final review |
| F-04 | P2 — migration text and the values table still taught the alpha constructor/body model | reviewer's `migration-snippet.log` | FIXED (contract table, feature comment) / docs branch (MIGRATION) | `e05d2f1`; M6 | final review |
| env | UI harness path normalization differs when `CARGO_HOME` sits inside the workspace | reviewer: all 51 UI cases pass from a relocated source-identical copy | OUT_OF_SCOPE (reviewer environment), mitigated: final review keeps `CARGO_HOME` outside the snapshot | — | — |

Contract-case rows reported FAIL by the reviewer (R09, T02, U29, U36) map to F-01, F-02, F-04;
NOT_RUN rows (C16, S10, T10, U20, U34, U35) and BLOCKED rows (T06 rendered QA, T08 Windows) are
carried into the acceptance record.
