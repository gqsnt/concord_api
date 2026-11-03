**CONFIRMED — C1 and C2 are resolved. No additional material finding in the changed records.** This confirms the records correction, not broader implementation acceptance.

Reviewed source: `6fed3aad3951cc211f2b152b536d4fdaa146cb0c`. Independent HEAD: `f7cef0894d10b2857407b0e396197479dab31757`.

Effective recorded settings are **`gpt-6.1-sol` / `xhigh`**, verified from this session’s [turn context](/home/canarit/.codex/sessions/2026/11/02/rollout-2026-11-02T03-47-57-01a19a28-0af6-77b0-8870-f8780cfe29b8.jsonl:10), with **read-only** sandbox and approval policy **never**. This verifies session metadata, not provider routing internals.

1. **Integrity and scope pass.** I independently hashed files, checked modes and reconstructed original trees for `b0fe26e` (**423 files**), `7b819cb` (**436**) and this source (**439**). Every result matches its manifest. All **223 original-source files outside `v4/`** are blob- and mode-identical to `b0fe26e`, with no additions or removals. Snapshot-generated `.concord-review.json` is separate metadata. Since `7b819cb`, only `FINAL.md`, `PERF.md`, `REVIEW_TRIAGE.md` and three archived confirmation files differ.

2. **C1 resolved.** Recomputed arithmetic supports [FINAL.md:125](/home/canarit/projects/concord_v4_refine_work/snapshots/confirm2-6fed3aa/v4/refinement/FINAL.md:125) and [PERF.md:46](/home/canarit/projects/concord_v4_refine_work/snapshots/confirm2-6fed3aa/v4/refinement/PERF.md:46):

   | Comparison | Raw recomputation |
   |---|---:|
   | Regressing release builds/rebuilds, noinline ÷ refined | 1.430840–2.285088× |
   | API-edit rebuild, `use1000` | 0.981422× |
   | Consumer-only time reduction, `use5` | 11.572396% |
   | Consumer-only time reduction, `use100` | 25.390503% |
   | Consumer-only time reduction, `use1000` | 16.766451% |
   | Consumer-only time reduction, fanout | 25.245480% |

   These support **1.43–2.29×**, the **0.98× exception**, and **12–25%** reductions. All **30 numeric table entries** in PERF §1 round correctly from raw samples. Check/dev changes span **+0.813–6.546%**, consistent with the approximate wording. The recorded symbol counts and generator agree on **200 copies / 150 distinct endpoints**, becoming **150 copies** without inline.

   I also recomputed all **114 hyperfine means**: each has three successful samples, and every stored mean matches. The other FINAL compile summaries round correctly. Its retained “+105 ns” runtime summary remains approximate: recorded paired differences are **111 and 103 ns**, averaging **107 ns**.

3. **C2 resolved.** [FINAL.md:153](/home/canarit/projects/concord_v4_refine_work/snapshots/confirm2-6fed3aa/v4/refinement/FINAL.md:153) now reflects the candidate/control evidence: key rotation succeeds, guides explain inherit/omit/replace, and doubled-`Option` confusion and the IP-base/subdomain mistake remain explicit. The nine-of-ten count is conservative: the task table gives T9 a second cycle for a runtime setup correction; the candidate separately says every task initially compiled.

4. **Other changed claims remain supported.** Archived prompt, receipt and report are byte-identical to the previous run’s originals; prompt/report SHA-256 values match the receipt. The revised sampling sentence correctly distinguishes compile samples from runtime summaries.

**Personally executed:** static hashing, mode/tree reconstruction, snapshot/archive comparisons, performance arithmetic, settings inspection and cleanliness checks. **Inspected only:** historical builds, tests, mutations, benchmarks, packaging and trial execution. No builds or tests were run. No edits, pushes, publications, global-setting changes or sandbox bypasses occurred. Git remains clean.