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
| P2-1 | High | generated `Debug` of a value used both as an open-trait header and as text bypassed the native flag | FIXED, then superseded | `76154bc` fixed the owner `Debug` (`DebugTextHeader`, mutation-checked); the executable review found the same value leaking through `Prepared`'s URL and a template header (F1 below), and the dual-use combination is now rejected unless statically sensitive, which removes `DebugTextHeader` |
| P2-2 | Medium | inserted `Content-Type` must carry the key's static classification | FIXED | contract 8f; `formats.rs::row8f_inserted_content_type_is_classified` (mutant M3 killed) |
| P2-3 | Medium | `content-type` precedence must be defined over composed, surviving contributions; four runtime cases; validation of unused constants; media-type syntax; cardinality | FIXED | contract 8a–8c, 8g; `row8b_four_runtime_cases`, `row8b_empty_value_empty_each_and_conversion_failure`, `ui/format_header_media_conflict.rs`, `row8c_remove_resolves_the_conflict`, `ui/format_media_type_invalid.rs` (mutant M4 killed) |
| P2-4 | Medium | rejecting inherited `-> ()` while accepting `-> Unit` makes acceptance spelling-dependent | FIXED (contract changed) | contract 3b; `row3b_3c_typed_unit_is_decoded`, `row5_aliases_follow_the_written_format` |
| P2-5 | Medium | feature/cfg promises need one strategy: alias gated by the union of consumer cfgs | FIXED | contract 12a–12b; `row12b_unused_and_cfg_disabled_defaults_are_never_referenced`, `ui_features/feature_disabled_formats.rs` (mutant M5 killed) |
| P2-6 | Medium | row 9 overpromised control over trait diagnostics (serde bounds) | FIXED (wording) | contract 9; `ui_json/format_serde_bound.rs` pins the actual message |
| P2-7 | Low | clearing rationale misstated inheritance | FIXED (wording) | `DECISIONS.md` N06, contract 6d, AUTHOR_GUIDE §5.2 |
| grammar | — | pin lexical precedence | FIXED | contract lexical table; `formats.rs::lexical_precedence`, `lexical_raw_identifier_is_a_payload`, `ui/format_reserved_words.rs` |

## Codex executable review of the candidate `66a4478`

- Reviewer: `gpt-6.1-sol`, effort `xhigh` (session `turn_context`), execute mode: workspace-write in
  the disposable snapshot `final-66a4478` (HEAD `e374f20`), network for loopback only, private
  `CARGO_HOME` outside the snapshot, fresh target directory. 2026-11-02 07:06–07:20 UTC. All origin
  files unchanged afterwards (receipt `final_66a4478_receipt.json`; the reviewer's own check: 654
  files byte-identical).
- Report: `final_66a4478_report.md` (sha256 `f75c5a92…`); probes, outputs and reconciliations in
  `final_66a4478_evidence/` (copies of the repository the reviewer made are not archived).
- Personally executed (per the report): fresh `verify.sh --msrv --package --nightly` (every gate
  but the package check, F2); 535 workspace tests; MSRV 416 / 96 / 11; nightly `Clone` 16; 31
  independent compiler probes on stable and 1.85; 7 independent contract/runtime probes (native
  bytes on the raw wire, flag preservation and OR-ing, borrowed statics, aliases, snapshots,
  request `Clone` with bodies, same-name order, owner `Debug`, absence/null/empty bodies,
  `Content-Type` precedence and sensitive insertion, drain vs typed unit, independent defaults,
  custom codecs, form in/out, unread status errors, decoding after native execution) plus the F1
  reproducer; four isolated mutations (all killed); 531 archived Criterion means reconciled;
  rustdoc in five feature configurations (HTML inspected as text; visual rendering blocked in the
  reviewer's sandbox — it was inspected by the coordinator in the in-app browser, `2c4ce3b`).
  Benchmarks not re-run. Verdict: **REVISE** (one P1, two P2, one P3; no P0).

| ID | Severity | Finding | Disposition | Fix / test |
|---|---|---|---|---|
| F1 | P1 | a custom `ToHeaderValue + Display` value whose header value is flagged, placed as a header and in a path, query and template: owner `Debug` redacted it, `Prepared`'s `Debug` printed it in the URL and in the template header | FIXED | `f2fb5a3`: such a value must be text itself (`TextHeader`) unless statically sensitive — a compile error at the type otherwise (`DECISIONS.md` N02 amendment, contract H10b); `ui/header_text_dual_use.rs` (mutation-checked), `native_headers.rs::dual_use_values_are_withheld_everywhere_or_are_text` |
| F2 | P2 | `package-check.sh` read `target/package` although `CARGO_TARGET_DIR` moved it | FIXED | `d78e567`: the target directory comes from `cargo metadata`; re-run with a foreign `CARGO_TARGET_DIR` |
| F3 | P2 | `migrate_via.py` turned `via codec::Text` into the built-in `text`, silently changing a downstream codec | FIXED | `d067cd7`: only `concord::`-qualified markers and bare names imported from `concord` become words; `--self-test` (9 cases, mutation-checked) in `verify.sh`; MIGRATION wording |
| F4 | P3 | `PERF.md` claimed ±1% intervals for every row and rlibs within 0.5% | FIXED (records) | two rows marked † (±43%, ±8.6%); rlibs +0.03% to +1.15%; figures recomputed from the archived summaries and `sizes.tsv` |
| cleanup | — | a reviewer documentation server (`127.0.0.1:44975`) outlived its session | RESOLVED | identity checked (started during the review, working directory the review snapshot, `http.server`), stopped by the coordinator |
| receipt | — | `evidence/verify_66a4478.txt` was absent from the reviewed snapshot | NOTED | it was committed after `66a4478` (`5e6d244`), so the snapshot could not contain it |

## Codex focused executable re-review of `1174d5d`

- Reviewer: `gpt-6.1-sol`, `xhigh` (session `turn_context`), execute mode in the fresh snapshot
  `rereview-1174d5d` (HEAD `bed9fd2`), loopback only, private `CARGO_HOME`, fresh target directory;
  2026-11-02 07:29–07:35 UTC; all 806 origin files unchanged; no server left running. Report
  `rereview_1174d5d_report.md` (sha256 `5c8329a2…`), probes in `rereview_1174d5d_evidence/`.
- Personally executed: the F1 reproducer and 40 compiler bypass cases per toolchain (optional and
  `each` values, `Box`/`Arc`/`Cow`/`&`, aliases, enclosing-owner references, separate endpoints,
  `override`/`append`, subdomains, sensitive key names, `#[cfg]`, renamed dependency) — all with
  the expected outcome; 9 runtime checks per toolchain (text controls, declared-sensitive dual
  use, owner/`Prepared` `Debug`, build/transport/status errors, raw-wire positive controls);
  package check with a foreign `CARGO_TARGET_DIR` and locked packaged consumers on stable,
  no-default and 1.85; migration reproducers; archive reconciliation for F4;
  `verify.sh --msrv --package` (all PASS; 535 / 416 / 96 / 11). Not run: benchmarks, nightly,
  visual rustdoc. Verdict: **REVISE** (F3 only).

| ID | Disposition | Evidence |
|---|---|---|
| F1 | RESOLVED | as above |
| F2 | RESOLVED | as above |
| F4 | RESOLVED | intervals 43.24% / 8.56%, rlibs +0.0306% to +1.1543%, consistent with the corrected record |
| F3 / F3a (P2) | NOT RESOLVED → FIXED in `b77084a` | the file-wide import scan ignored Rust scope, `cfg` and comments (`use concord::Text` in another module or a comment still mapped a downstream `Text`). Now only `concord::`-qualified markers become words; every other path is kept (same type). `migrate_via.py --self-test` includes the reviewer's scoped and commented imports (mutation-checked: mapping bare names again fails it); `formats.rs::migrated_marker_paths_behave_like_the_words` |

## Codex narrow executable check of the F3a fix (`d591fe7`)

- Reviewer: `gpt-6.1-sol`, `xhigh` (session `turn_context`; the report says it could not see
  them from inside), execute mode in the fresh snapshot `f3check-d591fe7`, loopback only, private
  `CARGO_HOME`; 2026-11-02 07:38–07:47 UTC; all 984 origin files unchanged; no server left.
  Report `f3check_d591fe7_report.md`; probes in `f3check_d591fe7_evidence/`. Executed on 1.97.1 and
  1.85.1: the four F3/F3a reproducers (pass), 38 paired wire checks of migrated declarations (30
  pass, 8 semantic failures), renamed-crate and re-export cases, absolute-path compiler checks,
  the self-test, the `formats` suite once. Verdict: **REVISE** — F3a RESOLVED, F3 not resolved.

| ID | Severity | Finding | Disposition | Fix / test |
|---|---|---|---|---|
| F3b | P2 | `concord::Text` was turned into the word even where `concord` is a local module or alias | FIXED | the script no longer turns any path into a word: it only moves the path (`migrate_via.py --self-test`, case with a local `mod concord`) |
| F3c | P2 | a downstream type named `text`/`json`/`bytes` kept verbatim lands in format position as the built-in word | FIXED | a bare lowercase name is left unmigrated and reported (self-test cases `via text`, `via r#json`); the compiler's leftover-`via` hint now names the path as written and `self::name` for a bare lowercase name, never a word (`ui/format_via_removed.stderr`, DIAGNOSTICS excerpt) |
| F3d | P3 | an absolute body format path (`::c4::Text b: T`) was rejected by the parameter lookahead | FIXED | `named` requires an identifier first; `formats.rs::migrated_marker_paths_behave_like_the_words` (`::concord::Text` in both positions; mutation: the old lookahead reproduces "expected identifier") |

