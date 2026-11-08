# Review l0-instrument-review-10-sol (INSTRUMENT re-review, I3 only, on 05413782)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-08T00:20:35Z–00:24:43Z, thread 01a1b887-484e-77d3-a7d0-0bba8de181bb.
**Verdict: REJECT** (1 blocking, 1 non-blocking). Confirmed: both review-8 holes fixed (the four
reproducers fail with E0107; the old diagnostic credits them); 172 positive controls, 138
expectations, 27 effect checks pass in the coordinator's run; no code injection (whole-type
parsing, expression/macro refusals, newline-separated insertion); include placement keeps
declaration-module resolution; projection refusal defensible (the question asks for resolved
types and the materials expose the policy `Output`); batched failures dropped independently;
earlier sandbox/env/socket/typed-binding/stream/call-form properties intact. Not executed by the
reviewer: builds, runners, bwrap; the new cases were assessed statically.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | Correct absolute paths refused: the function-local `extern crate self as <crate>` / `extern crate alloc` do not enter the extern prelude, so `::<crate>::Error` and `::alloc::…` fail although the unprefixed spellings pass (`score.py:267`); controls omit leading `::` | blocking | fix — I3 fix round 7: declare the aliases once at the scratch crate root under `cfg(test)`; absolute-path positive controls |
| 2 | `PhantomData<T>` is covariant: lifetime / higher-ranked subtyping can satisfy the check without identity; the judge permits higher-ranked fn types (`score.py:268`); no false credit on current lifetime-free truths, but "iff identical" is overstated | non-blocking | fix — same round: invariant marker (`PhantomData<fn(T) -> T>` or `*mut T`), higher-ranked negative control |
