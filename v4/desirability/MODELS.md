# Model and effort ledger — requested vs observed (from harness metadata: rollout turn_context, subagent transcripts, process args)

Append-only: a row marked `running` is superseded by a later row (`↳ … result` / `observed`) for the same role.

| Role / task | Requested | Observed (source) | Start–end (UTC) | Result |
|---|---|---|---|---|
| Coordinator | Claude Opus 5.5, xhigh | claude-opus-5-5, effort xhigh (desktop harness process args `--model=claude-opus-5-5 --effort=xhigh`) | 2026-11-07T19:11– | running |
| Codex smoke test (`work/roles/smoke`) | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context, thread 01a1b777-03b7) | 19:23 | OK |
| F2 docs drafting (CHANGELOG, MIGRATION) — Claude subagent a344e528 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 ×151 responses (subagent transcript message.model); effort not visible in transcript | 19:19:54–19:33:05 | drafted; coordinator reviewed; committed 7bd49dc4, 5523a2a0 |
| L0 canary lane 1 (`l0-canary-1-sol`) | Codex gpt-6.1-sol, xhigh, workspace-write + loopback | see work/roles/l0-canary-1-sol/meta.json | 19:4x– | running |
| F4 F-CLOSE review (`f4-fclose-sol`) on 5523a2a0 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 19:48:48–19:55:42 | ACCEPT_WITH_FIXES, 6 non-blocking; fixed in d5e796eb |
| L0 canary lane 1 (`l0-canary-1-sol`) | Codex gpt-6.1-sol, xhigh, workspace-write + loopback | gpt-6.1-sol / xhigh / workspace-write (rollout turn_context) | 19:37:48–19:48:53 | 10 suites; 5 leak candidates (1 real: Dropbox, fixed fac6e679) |
| L0 I3 build — Claude subagent | Claude subagent (inherits Opus 5.5) | (from transcript at completion) | 19:5x– | running |
| L0 canary lane 2 — Claude subagent | Claude subagent (inherits Opus 5.5) | (from transcript at completion) | 20:0x– | running |
| F4 recheck (`f4-fclose-recheck-sol`) on d5e796eb | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 19:56:47–19:58:07 | ACCEPT, no findings |
| L0 canary lane 2 — Claude subagent ad329fd0 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 ×205 responses (transcript); effort not visible | 20:0x–20:2x | 9 suites; 2 real leaks (Airtable, Shopify) fixed 20fce84f, a103ccdd |
| L0 canary lane 3 (`l0-canary-3-sol`) | Codex gpt-6.1-sol, xhigh, workspace-write + loopback + add-dir slot/roles | (meta.json at completion) | 20:2x– | running |
| L0 canary lane 3 (`l0-canary-3-sol`) | Codex gpt-6.1-sol, xhigh, workspace-write | gpt-6.1-sol / xhigh / workspace-write (rollout turn_context) | 20:17:54–20:29:47 | 9 suites, 4,980 renderings; Europe PMC identifier report triaged as not a leak |
| L0 canary lane 4 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 20:3x– | running |
| L0 I3 build — Claude subagent a389cf1d | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 19:5x–20:3x | built; committed fe959518 |
| I3 smoke readers (within I3 build) | claude-opus-5-5 (effort not passed); Codex gpt-6.1-sol xhigh read-only | claude-opus-5-5 (stream-json modelUsage); gpt-6.1-sol/xhigh/read-only (rollout) | 20:26:50–20:27:21 | 1.00 on all metrics (ceiling) |
| L0 I4 build — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 20:4x– | running |
| L0 INSTRUMENT review 1 (`l0-instrument-review-1-sol`) on 197bed2e | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 20:4x– | running |
| L0 INSTRUMENT review 1 (`l0-instrument-review-1-sol`) on 197bed2e | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 20:33:17–20:39:56 | REJECT: 6 blocking (R3 isolation, R3 semantics, R2 owner, material leakage, S3 signature, ClickHouse username), 3 non-blocking; triage in work/records/reviews/ |
| L0 canary lane 4 — Claude subagent a993c3e0 (+2 forks) | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (subagent report; transcript); effort not visible | 20:4x–21:0x | 9 suites, 7,058 renderings; USGS leak fixed 10f55efc |
| L0 I3 fix — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 21:1x– | running |
| F4 recheck 2 (`f4-recheck2-sol`) on 7eb093b4 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 21:02:28–21:03:25 | ACCEPT, no findings (Windows SystemTime diagnosis confirmed from std source) |
| L0 I4/I5 build — Claude subagent a8ab7444 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 20:4x–21:3x | 40 tasks validated; caller bank 30; committed 88b23cf2 |
| I4 smoke edit agent (codex.strava.e4, within I4 build) | Codex gpt-6.1-sol, xhigh, workspace-write | gpt-6.1-sol / xhigh / workspace-write (rollout turn_context) | ~4.5 min | oracle 4/4 |
| L0 canary lane 5 (`l0-canary-5-sol`) | Codex gpt-6.1-sol, xhigh, workspace-write | (meta.json at completion) | 21:2x– | running |
| L0 I3 fix — Claude subagent aeac9433 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 21:1x–21:5x | 4 blocking fixed with self-test evidence; committed f8097095 |
| L0 INSTRUMENT review 2 (`l0-instrument-review-2-sol`) on 343fec74 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 21:5x– | running |
| L0 canary lane 5 (`l0-canary-5-sol`) | Codex gpt-6.1-sol, xhigh, workspace-write | gpt-6.1-sol / xhigh / workspace-write (rollout turn_context) | 21:22:13–21:32:07 | 9 suites, 4,411 renderings, no leak; committed c66c04a2 |
| L0 canary lane 6 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 22:0x– | running |
| L0 INSTRUMENT review 2 (`l0-instrument-review-2-sol`) on 343fec74 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 21:31:08–21:37:46 | REJECT: 5 blocking (R3 compile/IPC escape, R3 result observability, R2 nested prefix, I4 oracle isolation, I4 oracle strength), 3 non-blocking |
| L0 I3 fix round 2 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 22:1x– | running |
| L0 canary lane 6 — Claude subagent abe393f2 (+6 forks) | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (report/transcript); effort not visible | 22:0x–22:2x | 9 suites, 9,219 renderings; Salesforce SOQL text fixed db3b2e39 |
| L0 I4 fix round — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 22:3x– | running |
| L0 I3 fix round 2 — Claude subagent a735df50 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (report/transcript); effort not visible | 22:1x–22:4x | 3 blocking fixed, self-test all met; committed 984bd2f6 |
| L0 INSTRUMENT review 3 (I3 only, `l0-instrument-review-3-sol`) on 78571132 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 22:4x– | running |
| L0 INSTRUMENT review 3 (I3, `l0-instrument-review-3-sol`) on 78571132 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 22:11:23–22:16:56 | REJECT: early return, stream items, R2 module erasure |
| L0 I3 fix round 3 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 23:2x– | running |
| L0 I4 fix round — Claude subagent a9b0d142 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (report/transcript); effort not visible | 22:3x–23:2x | isolation + 24 oracles + 17 controls; committed 7f05872e |
| I4 isolated smoke edit agent (strava/e2) | Codex gpt-6.1-sol, xhigh, danger-full-access inside bwrap | gpt-6.1-sol / xhigh / danger-full-access (rollout turn_context) | ~3 min | oracle 2/2, valid |
| L0 INSTRUMENT review 4 (I4, `l0-instrument-review-4-sol`) on d8327180 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 23:3x– | running |
| L0 INSTRUMENT review 4 (I4, `l0-instrument-review-4-sol`) on d8327180 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 22:40:14–22:45:20 | REJECT: 5 blocking (git config escape, public bank, runner swap, Kraken self-check, query order) |
| L0 I4 fix round 2 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 23:5x– | running |
| L0 I3 fix round 3 — Claude subagent aa27cd7d | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 23:2x–23:5x | 3 blocking + 1 fixed, self-test all met; committed 05be505a |
| L0 INSTRUMENT review 5 (I3, `l0-instrument-review-5-sol`) on 468436c6 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 23:5x– | running |
| L0 INSTRUMENT review 5 (I3, `l0-instrument-review-5-sol`) on 468436c6 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 22:47:00–22:53:26 | REJECT: byte-literal lexer bypass; sdk:: canonical spoofing |
| L0 I3 fix round 4 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 00:0x– | running |
| L0 I3 fix round 4 — Claude subagent a2623949 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 00:0x–00:2x (11-08) | syn parser + tagged identities; committed d8c57b87 |
| L0 INSTRUMENT review 6 (I3, `l0-instrument-review-6-sol`) on 225e73b1 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 00:2x– | running |
| L0 I4 fix round 2 — Claude subagent af1f0cd0 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 23:5x–00:3x | sealed bank + trusted measurement + oracle_verdict; committed 61748f17 |
| I4 sealed-runner smoke edit agent (kraken e4) | Codex gpt-6.1-sol, xhigh, danger-full-access inside bwrap | gpt-6.1-sol / xhigh / danger-full-access (rollout turn_context) | 2m12s | valid, oracle 3/3 executed |
| L0 INSTRUMENT review 7 (I4, `l0-instrument-review-7-sol`) on 7f5a83e6 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 00:3x– | running |
| L0 INSTRUMENT review 6 (I3, `l0-instrument-review-6-sol`) on 225e73b1 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 23:16:53–23:20:55 | REJECT: std/core/alloc interchange; malformed types repaired |
| L0 I3 fix round 5 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 00:4x– | running |
| L0 INSTRUMENT review 7 (I4, `l0-instrument-review-7-sol`) on 7f5a83e6 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 23:19:18–23:24:05 | REJECT: symlink overwrite, forgeable oracle, Kraken self-check |
| L0 I4 fix round 3 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 00:5x– | running |
| L0 T1 docs-truth review (`t1-review-sol`) on 8c97e367 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 01:1x– | running |
| L0 T1 docs-truth review (`t1-review-sol`) on 8c97e367 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 23:31:39–23:34:00 | ACCEPT_WITH_FIXES (1 wording fix applied 32fc4312) |
| L0 I3 fix round 5 — Claude subagent afcac11b | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 00:4x–01:0x | syn types + std table; committed b77c9cef |
| L0 INSTRUMENT review 8 (I3, `l0-instrument-review-8-sol`) on 6638e5b6 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 01:2x– | running |
| L0 INSTRUMENT review 8 (I3, `l0-instrument-review-8-sol`) on 6638e5b6 | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context) | 23:42:05–23:44:48 | REJECT: lifetime args discarded; alias surplus args dropped |
| L0 I3 fix round 6 — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 01:3x– | running |
| L0 I4 fix round 3 — Claude subagent ae1fa9b2 | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | 00:5x–01:3x | 5 findings fixed; committed 9fb2423d; bank af768919… |
| I4 fix3 smoke edit agent (kraken e4) | Codex gpt-6.1-sol, xhigh, danger-full-access inside bwrap | gpt-6.1-sol / xhigh / danger-full-access (rollout turn_context) | 2m46s | valid, oracle 3/3 |
| L0 INSTRUMENT review 9 (I4, `l0-instrument-review-9-sol`) on f81caeac | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 01:4x– | running |
| ↳ review 9 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 23:57:26Z–00:01:12Z | 3m46s | ACCEPT_WITH_FIXES (2 non-blocking) |
| L0 I4 fix round 4 — Claude subagent (general-purpose) | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 01:0x– | running |
| L0 I3 fix round 6 — Claude subagent (general-purpose) | Claude subagent (inherits Opus 5.5) | claude-opus-5-5 (transcript); effort not visible | ~33 min, 97 tool uses | self-test 138/138 + 27 effects; committed 1708a9ad (with coordinator reader-text sentence) |
| L0 INSTRUMENT review 10 (I3, `l0-instrument-review-10-sol`) on 05413782 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | 01:3x– | running |
| ↳ I4 fix round 4 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~18 min, 74 tool uses | validate 0 diffs/40; committed eb078a78, merged 66edaeff; bank e7d963b9… |
| L0 INSTRUMENT review 11 (I4 fix4, `l0-instrument-review-11-sol`) on 66edaeff | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ review 10 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 00:20:35Z–00:24:43Z | 4m08s | REJECT (1 blocking: absolute paths; 1 non-blocking: variance) |
| ↳ review 11 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 00:21:29Z–00:25:07Z | 3m38s | ACCEPT_WITH_FIXES (2 non-blocking) |
| L0 I3 fix round 7 — same Claude subagent resumed (SendMessage) | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| L0 I4 fix round 5 — same Claude subagent resumed (SendMessage) | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ I3 fix round 7 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~9.5 min, 32 tool uses | self-test 154 + 29 effects; committed ab405ce8, merged 8dc50be7 |
| L0 INSTRUMENT review 12 (I3, `l0-instrument-review-12-sol`) on 8dc50be7 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ review 12 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 00:35:26Z–00:39:24Z | 3m58s | ACCEPT_WITH_FIXES (2 non-blocking); I3 ACCEPTED |
| L0 I3 fix round 8 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ I4 fix round 5 result | — | claude-opus-5-5 (subagent transcript); effort not visible | interrupted by host restart ~05:17, resumed, finished | validate 0 diffs/40; bank unchanged e7d963b9; committed c02c6659, merged cfa1e41f |
| ↳ I3 fix round 8 result | — | claude-opus-5-5 (subagent transcript); effort not visible | interrupted by host restart before edits, resumed, ~9 min | self-test 154 + 32 effects; committed f1792fed, merged bc997a04 |
| L0 INSTRUMENT review 13 (I3 fix8 + I4 fix5 + I1 reachability, `l0-instrument-review-13-sol`) on bc997a04 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ review 13 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 04:28:32Z–04:33:48Z | 5m16s | ACCEPT_WITH_FIXES (4 non-blocking); 3 recorded as limits, 1 fixed by coordinator eb46bf2b |
