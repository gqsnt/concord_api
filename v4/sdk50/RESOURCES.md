# Resource ledger

Machine: 20 CPUs, 30 GiB RAM, 7 GiB swap; `/` ext filesystem 915 GB (nvme0n1p2), shared with the user's other projects. Floor: 20 GiB free. Concurrency: at most one full gate and two heavy jobs; Codex lanes build with `CARGO_BUILD_JOBS=4–6`.

| Time | Measurement |
|---|---|
| 2026-11-06 23:04 | 529 GB free before any campaign build |
| 2026-11-06 23:15 | M0 cold full gate (`--package --bench`): 10m00s; target 63 GB; 467 GB free after |
| 2026-11-07 00:08 | integration preview full gate: 9m12s; target 65 GB; 393 GB free |
| 2026-11-07 00:20 | main `8cae5d4c` full gate reusing the M0 target (identical tree): 3m39s |

## Pruned (reproducible cargo output of merged, verified lanes; no process using them)

- 2026-11-06T23:58:10+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/p0-docs/target (1.1G) reason: merged lane, reproducible cargo output
- 2026-11-06T23:58:10+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/p1-link/target (4.1G) reason: merged lane, reproducible cargo output
- 2026-11-06T23:58:10+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/p1-f2-adoption/target (12G) reason: merged lane, reproducible cargo output
- 2026-11-06T23:58:10+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/p1-codecs/target (14G) reason: merged lane, reproducible cargo output
- 2026-11-06T23:58:11+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/targets/f2-adoption (11G) reason: merged lane, reproducible cargo output
- 2026-11-06T23:58:11+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/targets/probe-desk (997M) reason: merged lane, reproducible cargo output
- 2026-11-06T23:58:11+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/targets/p1 (11G) reason: merged lane, reproducible cargo output
- 2026-11-06T23:58:13+01:00 pruned /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/targets/integration (30G) reason: merged lane, reproducible cargo output

- 2026-11-07T04:47 CET, **owner approved in chat**: 25 merged lane targets (`b12-*`, `b23-*`, `p1-docs2`, `p1-edit-k8s`, `p1-pagination`, `p1-retry`, `p2-auth`, `p2-ratelimit`, `w1-*`, `w2-*` under `work/`), ≈97 GB; each lane merged into `sdk50/integration`, clean tree, no process inside; free space 164 → 259 GB. Per-directory sizes in `work/records/RESOURCE_LEDGER.log`. (An earlier prune attempt had been refused by Claude Code's `rm` check.)

## Kept

- `work/m0/target` (M0/main verification), `work/integration/target` (integration gate), active lane targets.
- Logs: `work/logs/` (outside the repository); role transcripts: `work/roles/<id>/`.
