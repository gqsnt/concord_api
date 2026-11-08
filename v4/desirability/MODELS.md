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
| BASELINE I3 readers run 1+2 (`work/baseline/i3/runs-claude-{1,2}`) on 734991e9 | Claude `claude -p` claude-opus-5-5, no tools, effort not passed (default) | (per-session transcripts: message.model / modelUsage) | 05:4x– | running |
| BASELINE I3 readers run 1+2 (`work/baseline/i3/runs-codex-{1,2}`) on 734991e9 | Codex gpt-6.1-sol, xhigh, read-only, empty dir | (per-session rollout turn_context) | 05:4x– | running |
| ↳ BASELINE I3 Claude readers observed | — | claude-opus-5-5 in 128/128 sessions (transcript message.model / modelUsage); effort not visible; 0 tool calls, 0 retries | run1 134s, run2 130s | R1.critical 0.92/0.92, R1.exact 0.86/0.86, R2 1.00, R3 1.00 |
| ↳ BASELINE I3 Codex readers observed | — | gpt-6.1-sol / xhigh / read-only in 128/128 sessions (rollout turn_context); 0 tool calls, 0 retries | run1 192s, run2 186s | R1.critical 1.00/0.95, R1.exact 0.94/0.90, R2 1.00, R3 1.00 |
| BASELINE I4 edit agents, 40 sealed tasks (`work/baseline/i4/runs`) on 734991e9 | Codex gpt-6.1-sol, xhigh, danger-full-access inside bwrap | (per-run result.json: rollout turn_context) | 06:0x– | running |
| H04 instrument lane (sealed task bank + scorer) — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 06:1x– | running |
| ↳ H04 instrument lane result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~50 min, 126 tool uses | 24 tasks / 66 facts sealed (bank 2c9cae59); validation 24/24, 66/66, 24/24, selftest 23/23; tooling c0839cad (local) |
| H04 INSTRUMENT review 1 (`h04-instrument-review-1-sol`) on c0839cad | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 review 1 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 05:43:07Z–05:49:19Z | 6m12s | REJECT (8 blocking) |
| H04 instrument fix round 1 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ BASELINE I4 edit agents observed | — | gpt-6.1-sol / xhigh / danger-full-access inside bwrap in 40/40 rollouts (turn_context); isolation check passed before each launch | agent median 120 s (82–287 s); 47 min wall | 40/40 valid; oracle 39/40 |
| ↳ BASELINE review (`l0-baseline-review-sol`) on 7ac81a4d | Codex gpt-6.1-sol, xhigh, read-only | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 06:01:17Z–06:10:19Z | 9m02s | REJECT (2 blocking: noise-band rounding, I4 propagation reading; 4 non-blocking) — all fixed |
| BASELINE re-review (`l0-baseline-review-2-sol`) on b2af3043 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ BASELINE re-review observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 06:12:05Z–06:16:18Z | 4m13s | ACCEPT (no findings); L0 complete |
| ↳ H04 instrument fix round 1 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~32 min, 78 tool uses; isolation probe: 1 dummy session per provider | validation all pass; committed c8c82534 (local); bank e68bee14 |
| H04 INSTRUMENT review 2 (`h04-instrument-review-2-sol`) on c8c82534 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 review 2 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 06:23:49Z–06:29:57Z | 6m08s | REJECT (6 blocking) |
| H04 instrument fix round 2 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ H04 instrument fix round 2 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~15 min, 29 tool uses; isolation probes with a dummy prompt | validation all pass; committed e50a22ae (local); bank f21f432e |
| H04 INSTRUMENT review 3 (`h04-instrument-review-3-sol`) on e50a22ae | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 review 3 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 06:45:47Z–06:50:58Z | 5m11s | REJECT (3 blocking) |
| H04 instrument fix round 3 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ H04 instrument fix round 3 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~28 min, 120 tool uses; no model calls | validation all pass; committed 81618089 (local); bank 4b93f872 |
| H04 INSTRUMENT review 4 (`h04-instrument-review-4-sol`) on 81618089 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 review 4 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 07:20:00Z–07:24:28Z | 4m28s | REJECT (1 blocking) |
| H04 instrument fix round 4 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| H04 candidate implementer (generator docs, `dz/h04`) — Claude subagent | Claude subagent (inherits Opus 5.5) | (transcript at completion) | 08:3x– | running |
| ↳ H04 instrument fix round 4 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~23 min, 64 tool uses; isolation probe 1 dummy session per provider (observed claude-opus-5-5; gpt-6.1-sol/xhigh) | validation all pass; committed c383065a (local); bank 636a6738 |
| H04 INSTRUMENT review 5 (`h04-instrument-review-5-sol`) on c383065a | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 review 5 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 07:48:51Z–07:52:42Z | 3m51s | REJECT (1 blocking: backstop bypass; 1 non-blocking: card hash) |
| H04 instrument fix round 5 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ H04 instrument fix round 5 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~6 min, 20 tool uses; no model calls | gate selftest 11/11; committed 9ce64a00 (local); bank unchanged 636a6738 |
| H04 INSTRUMENT review 6 (`h04-instrument-review-6-sol`) on 9ce64a00 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 review 6 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 07:59:23Z–08:04:31Z | 5m08s | REJECT (2 blocking: moves/deletions, normalisation) |
| H04 instrument fix round 6 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ H04 instrument fix round 6 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~5 min, 19 tool uses; no model calls | gate selftest 29/29; committed a3fceb08 (local); bank unchanged |
| H04 INSTRUMENT review 7 (`h04-instrument-review-7-sol`) on a3fceb08 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 candidate implementer result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~45 min, 196 tool uses | generator-only; API identical without docs (64 crates); expanded code identical (10 SDKs); committed 4145921f on dz/h04 (pushed for CI); boundary audit clean |
| H04 instrument fix round 7 (stale per-arm rustdoc, found in use) — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ H04 review 7 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 08:10:29Z–08:14:52Z | 4m23s | REJECT (4 blocking: prompt integrity) |
| ↳ H04 instrument fix round 7 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~10 min, 31 tool uses; no model calls | committed dcf99c21 (local); render control ok; e2e 22/22 |
| H04 INSTRUMENT review 8 (`h04-instrument-review-8-sol`) on dcf99c21 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| H04 CANDIDATE review 1 (`h04-candidate-review-1-sol`) on 4145921f | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 candidate review 1 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 08:24:12Z–08:28:12Z | 4m00s | ACCEPT; clearance CLEAR ×5 |
| ↳ H04 review 8 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 08:24:12Z–08:30:30Z | 6m18s | REJECT (2 blocking: run integrity) |
| H04 instrument fix round 8 — same Claude subagent resumed | Claude subagent (inherits Opus 5.5) | (transcript at completion) | running | — |
| ↳ H04 instrument fix round 8 result | — | claude-opus-5-5 (subagent transcript); effort not visible | ~6 min, 17 tool uses; no model calls | committed 6ede0339 (local); bank 06be1a2b; e2e 32/32 |
| H04 INSTRUMENT review 9 (`h04-instrument-review-9-sol`) on 6ede0339 | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 review 9 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 08:37:24Z–08:41:15Z | 3m51s | ACCEPT (no findings); H04 instrument accepted |
| H04 pilot readers, 72 sessions (`work/sealed/h04-runs/pilot-2`), arms 1d519c73 vs 4145921f | Claude `claude -p` claude-opus-5-5 (no tools, no setting sources) and Codex gpt-6.1-sol xhigh read-only, both in bubblewrap | (per-session transcripts / rollouts) | 09:0x– | running |
| ↳ H04 pilot readers observed | — | Claude claude-opus-5-5 36/36 (effort not visible); Codex gpt-6.1-sol/xhigh 36/36; 0 retries, 0 malformed | sessions ~1h | witness net +7, control lost 1 → WIN_PENDING_GATES |
| H04 DECISION review (`h04-decision-review-sol`) | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 decision review observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 08:47:55Z–08:54:16Z | 6m21s | REJECT; INCONCLUSIVE (clearance unsubstantiated) — pilot-2 non-qualifying |
| H04 CLEARANCE review 2 (`h04-clearance-review-2-sol`, full diffs, pilot-3) | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion; rollout checked for complete reads) | running | — |
| ↳ H04 clearance review 2 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 08:55:34Z–08:58:52Z | 3m18s | CLEAR ×5; full reads verified from rollout |
| H04 pilot-3 readers, 72 sessions (`work/sealed/h04-runs/pilot-3`, seed 5) | Claude claude-opus-5-5 / Codex gpt-6.1-sol xhigh, bubblewrap | (per-session metadata) | running | — |
| ↳ H04 pilot-3 readers observed | — | Claude claude-opus-5-5 36/36 (effort not visible); Codex gpt-6.1-sol/xhigh/read-only 36/36; 0 retries, 0 malformed | 249 s wall | witness net +6, control lost 0 → WIN_PENDING_GATES |
| H04 DECISION review 2 (`h04-decision-review-2-sol`, pilot-3) | Codex gpt-6.1-sol, xhigh, read-only | (meta.json at completion) | running | — |
| ↳ H04 decision review 2 observed | — | gpt-6.1-sol / xhigh / read-only (rollout turn_context); 09:05:17Z–09:11:04Z | 5m47s | ACCEPT_WITH_FIXES; WIN_PENDING_CI |
