# H16 pre-card deficit check (current head, 2026-11-09)

I3 R1 (`wire` sessions) of kubernetes, mediawiki-action-api, nager-holidays, fred, clickhouse, openai,
backblaze-b2-native, world-bank, pokeapi, frankfurter on `f921b002` (truth regenerated; prompts packed);
readers Codex `gpt-6.1-sol` xhigh (2 runs) and Claude `claude-opus-5-5` effort medium (1 run; owner
A3.2). Script `work/tools/h16_deficit_check.sh`; runs and scores `work/dev/h16/i3-current/`.

| SDK | items | R1.critical (Codex 1 / Codex 2 / Claude) | R1.exact (Codex 1 / Codex 2 / Claude) |
|---|---|---|---|
| kubernetes | 6 | 6 / 3 / 3 | 6 / 3 / 3 |
| mediawiki-action-api | 3 | 3 / 3 / 0 | 3 / 3 / 0 |
| nager-holidays | 1 | 1 / 1 / 0 | 1 / 1 / 0 |
| fred | 6 | 6 / 6 / 6 | 4 / 4 / 4 |
| clickhouse | 1 | 1 / 1 / 1 | 0 / 0 / 0 |
| openai | 3 | 3 / 3 / 3 | 2 / 2 / 2 |
| backblaze-b2-native | 3 | 3 / 3 / 3 | 2 / 2 / 3 |
| world-bank (contrary) | 1 | 1 / 1 / 1 | 1 / 1 / 1 |
| pokeapi (control) | 2 | 2 / 2 / 2 | 2 / 2 / 2 |
| frankfurter (control) | 1 | 1 / 1 / 1 | 1 / 1 / 1 |

Witness R1.exact (23 items): 18 / 15 / 12. Measured before the H16 card was written; the card's
thresholds were then set with this baseline in view (it is a baseline, not a candidate outcome).
