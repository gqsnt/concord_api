# H04 instrument — coordinator decisions on the implementer's questions (not a review)

Implementer: Claude subagent (Opus 5.5), tooling `c0839cad` (local `dz/h04-bank`), bank
`2c9cae59…` sealed in `work/sealed/h04-bank/`.

1. Decision-rule readings kept: control losses count regressions (the card: "controls lose ≤ 2
   of their 72 answers"); "recurrent" = ≥ 2 regressions in one SDK/task or one control fact; gate
   outcome, increased caller obligations and attribution are coordinator inputs; WIN shown as
   WIN_PENDING_GATES until gates are known. (Put to review 1.)
2. Materials kept as built: public rustdoc only (what a caller sees), no `api!` declaration (the
   card: docs only), Rust code examples replaced in both arms (A1 §6: no answer-bearing
   examples), prose kept. Consequence recorded as a limit: the pilot does not measure the value of
   worked examples.
3. Runs and prompts go under `work/sealed/h04-runs/` (prompts carry sealed task text).
4. Claude readers: `--claude-model claude-opus-5-5` (as in the I3 baseline); Codex `gpt-6.1-sol`
   xhigh read-only.
5. Both arms rendered with the same nightly (1.101.0-nightly 2026-10-12); re-render both if it
   changes.
6. Expected ceilings (Sentry/Strava result-type facts) noted; they are preservation facts.
