# Cross-50 analysis — read-only evidence sweep

You are one of six read-only analysts. The working directory is a git worktree of the Concord
workspace (`sdk50/integration`), containing Concord (`crates/concord`, `crates/concord-core`,
`crates/concord-macros`, `crates/concord-codecs`, `crates/concord-pagination`,
`crates/concord-retry`, `crates/concord-auth`, `crates/concord-ratelimit`) and 55 SDKs built on it
(`examples/*-sdk`, each with `src/`, tests, and usually `README.md`, `FRICTION.md`, `FIRST_USE.md`
from a fresh consumer, and `MAINTENANCE.md` from an independent maintainer). Do not build or run
anything; read code and documents. Read nothing outside the worktree. No external service.

The owner's goal: not an ultimate toolkit — the cleanest, simplest API, interface and abstraction,
so that everything integrates well and is well represented; any change is judged across all the
domains these SDKs cover. Concord's fixed organizing model is client → scope → endpoint.

For your theme:
1. Sweep **every** SDK (not a sample). For each one where the theme appears, give: file:line(s),
   what the code does, how many lines it costs, and whether the SDK's FRICTION / FIRST_USE /
   MAINTENANCE notes mention it. Mark SDKs where the theme is absent.
2. Group the occurrences into distinct mechanics (do not merge different things).
3. For each mechanic, state the smallest Concord change that would remove it, where it belongs
   (core / macros / an existing crate / docs only / leave to SDKs), how many SDKs and lines it would
   remove, what public API it would add **or remove**, and the counterexamples: SDKs where it would
   not fit, would add code, or would weaken a guarantee.
4. Also list what Concord could **remove or shrink** under your theme: features, generated items or
   crate APIs that few or no SDKs use, or that SDKs hide or work around.
5. Separate observed facts (with file:line) from inference.

Output: a report, most valuable findings first, with the per-SDK table, the mechanics, the
candidate changes (add / remove / docs), counterexamples, and what you could not decide.
