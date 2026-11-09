# h09a-baseline-review-sol (Codex gpt-6.1-sol xhigh, read-only; observed gpt-6.1-sol xhigh; 2026-11-09T03:03Z–03:12Z; independent session, same provider as the Codex-authored bank lane)

VERDICT: REJECT
DISPOSITION: CARD — Test whether explicit qualified `Resume` import guidance reduces observed resume compile failures; smallest comparator: current `_from` API with linked docs versus the same API with that guidance.

FINDINGS:

- [blocking] Measured failures dismissed without testing the smallest comparator — v4/desirability/measurements/H09a-baseline.md:55 — Codex resume success is 37/42 versus fresh 33/33, including failures in all three GitLab repetitions. These are measured ordinary-use failures. Provider disagreement limits generalization, while the 9/12 re-export census supplies context rather than independently establishing a defect. The evidence supports a bounded import-discovery card; it does not support `Pages::starting_at` or twin removal. Fix by narrowing the claim and pre-registering a docs-first comparison, with SDK re-exports considered only if needed.
- [non-blocking] Provenance wording overstates unread inputs — /home/canarit/projects/Concord_Desirability_Loop/work/sealed/h09a-bank/prepare_linked.py:12 — `load_set` parses task files, and the unchanged navigation filter reads SDK source. The appended content nevertheless reproduces entirely from public rustdoc. Clarify that task/key/source content does not supply the additions; another materials round is unnecessary.

CHECKED: Read the governing rules and triage standard. Independently recounted both score files. Round 2: resume Codex 37/42, Claude 42/42; fresh 33/33 each; edge 3/3 each; supplement resume 2/3 and 3/3, fresh 3/3 each; PokeAPI resume 3/3 each, fresh 0/3 and 3/3. Excluding `h09a/notion/02` is justified: one required value is absent from its text and packed prompt. All five scored Codex resume misses are the invented `concord::pagination::Resume` path; the supplement has one additional miss.

V1 omits `Resume`’s definition in all 12 twin SDKs; all 15 main resume references require `Resume::At`. Every round-1 main/supplement resume failure is a compile failure consistent with that omission. This is verdict-changing instrument evidence under A3. Round 2 legitimately corrects it before disposition within the two-round cap.

Verified public signature links, same-commit rustdoc provenance, unchanged H04 renderer/filter, and exact reproduction of appended content. All 39 files retain their v1 prefix; CHEATSHEET and PokeAPI remain identical. Reconstructed all packed prompts; verified hashes, template, seed, identical schedule, frozen bank hash, 122/122 recorded validation checks, session metadata/logs, and the 9/12 re-export census.

NOT CHECKED: Runtime reproduction, builds, readers, sandbox execution and CI were prohibited. Live docs.rs navigation was not exercised. No candidate comparison or human-use evidence establishes that import guidance or re-exports improve outcomes; the proposed card must test that.