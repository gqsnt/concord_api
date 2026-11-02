# Review triage (native headers and formats campaign)

Raw reports are archived unedited in `reviews/codex/`. Dispositions: CONFIRMED, FIXED,
REJECTED_WITH_EVIDENCE, DESIGN_TRADEOFF, OUT_OF_SCOPE, BLOCKED. A reviewer never certifies its
own implementation: every reviewed implementation here was written by Claude agents and reviewed
by Codex.

## Codex design challenge pass 1 (blind)

- Reviewer: Codex CLI 0.166.0, `gpt-6.1-sol`, effort `xhigh` — observed in the session
  `turn_context` (the report itself says it could not observe them from inside). Execute mode:
  workspace-write in the disposable snapshot of `a915c8d` (`codex-design-a915c8d`), network enabled
  for loopback only, private `CARGO_HOME` outside the snapshot. 2026-11-02 03:36–03:52 UTC. Source
  integrity after the run: all origin files unchanged (receipt `design_pass1_receipt.json`).
- Inputs: the owner charter, the comparison corpus and the source. It saw no local proposal.
- Report: `design_pass1_report.md` (sha256 `cc73e652…`). Personally executed (per the report): 54
  header coherence checks, 8 DSL diagnostics, 6 native-placement negatives on 1.97.1 and 1.85; the
  20-endpoint corpus in three designs with shared loopback wire assertions; 311 existing tests;
  two baseline → mutant → restored experiments. Multipart not verified (offline cache lacked
  `mime_guess`).
- Recommendations and dispositions:

| Point | Disposition |
|---|---|
| Concrete header-input trait, reference forwarding, no `Display` blanket (E0119) | ADOPTED (N01; Track H reached the same boundary independently) |
| `Format<T>` wrapper as the formatting escape | REJECTED_WITH_EVIDENCE (N01): a transport wrapper in caller signatures; the template `"{v}"` keeps the caller's type |
| Sensitivity = native flag OR declaration/name classification, per value | ADOPTED (N02) |
| Split the ASCII policy by source | ADOPTED (N03) |
| Literal templates → validated statics | ADOPTED (N04) |
| Owner defaults (A) over local annotations (B) | ADOPTED in combination (N06): defaults on owners, exceptions in place |
| `request -;` / `response -;` clearing | REJECTED_WITH_EVIDENCE (N06): no corpus case; guidance instead |
| Trap list (P0–P2) | each trap has a test in the implementation (see `CONTRACT.md` rows and `TEST_MATRIX.md` H/F rows) |

## Codex design challenge pass 2 (the selected design, static)

- Reviewer: `gpt-6.1-sol`, `xhigh` (session `turn_context`), read-only sandbox, snapshot
  `codex-pass2-93cf892` of `93cf892`. 2026-11-02 04:16–04:21 UTC. Inputs: the charter,
  `DECISIONS.md`, `CONTRACT.md` as first written, and the three track reports. Report
  `design_pass2_report.md` (sha256 `9c93382f…`). Executed: git identity and manifest checks only
  (no compilation). Verdict: **ADOPT WITH CHANGES**.

| ID | Severity | Finding | Disposition | Fix / test |
|---|---|---|---|---|
| P2-1 | High | generated `Debug` of a value used both as an open-trait header and as text bypassed the native flag | FIXED | `76154bc`: `Value::header`, `DebugTextHeader(Each)`; `native_headers.rs::debug_withholds_a_flagged_header_value_also_used_as_text` (mutation-checked: route back to `DebugDisplay` → fails) |
| P2-2 | Medium | inserted `Content-Type` must carry the key's static classification | FIXED | contract 8f; `formats.rs::row8f_inserted_content_type_is_classified` (mutant M3 killed) |
| P2-3 | Medium | `content-type` precedence must be defined over composed, surviving contributions; four runtime cases; validation of unused constants; media-type syntax; cardinality | FIXED | contract 8a–8c, 8g; `row8b_four_runtime_cases`, `row8b_empty_value_empty_each_and_conversion_failure`, `ui/format_header_media_conflict.rs`, `row8c_remove_resolves_the_conflict`, `ui/format_media_type_invalid.rs` (mutant M4 killed) |
| P2-4 | Medium | rejecting inherited `-> ()` while accepting `-> Unit` makes acceptance spelling-dependent | FIXED (contract changed) | contract 3b; `row3b_3c_typed_unit_is_decoded`, `row5_aliases_follow_the_written_format` |
| P2-5 | Medium | feature/cfg promises need one strategy: alias gated by the union of consumer cfgs | FIXED | contract 12a–12b; `row12b_unused_and_cfg_disabled_defaults_are_never_referenced`, `ui_features/feature_disabled_formats.rs` (mutant M5 killed) |
| P2-6 | Medium | row 9 overpromised control over trait diagnostics (serde bounds) | FIXED (wording) | contract 9; `ui_json/format_serde_bound.rs` pins the actual message |
| P2-7 | Low | clearing rationale misstated inheritance | FIXED (wording) | `DECISIONS.md` N06, contract 6d, AUTHOR_GUIDE §5.2 |
| grammar | — | pin lexical precedence | FIXED | contract lexical table; `formats.rs::lexical_precedence`, `lexical_raw_identifier_is_a_payload`, `ui/format_reserved_words.rs` |

## Codex executable review of the candidate

Pending — recorded here when it has run.
