# Independent review: two between-waves changes (waves 3 → 4)

Read-only review. Do not build or run tests; read code, tests and docs. No external service.

1. `git -C /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-retry show 254a5381`
   — `concord-retry`: `Retry::receive_with`, an async fallible before-attempt hook that owns the
   request and returns `Result<(Request, A), E>`, with context `A` passed to classification and
   dropped before backoff; `ReceiveRun`, `ReceiveReport { retry, completed, refused, no_response }`;
   PayPal's adoption (`examples/paypal-sdk/src/policy.rs`). Check: can a refused attempt be
   dispatched anyway, a context outlive its attempt across backoff, a deadline be exceeded by
   hooks, an accounting count be wrong (dispatches vs completions vs refusals vs no-response),
   a one-shot body be replayed, or PayPal's replay/uncertainty rules change (compare with the
   parent commit)? Are `receive`/`run` behaviour and their reports unchanged?
2. `git -C /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-core show 32e7b0a7`
   — generated endpoint docs (crates/concord-macros/src/emit.rs): wire shape first, labelled base
   URL, value-policy wording, declared-result wording. Check that every generated doc line is
   still emitted exactly once and still true for every endpoint kind (drain, stream, typed,
   native headers, value and type policies, with and without `send P -> Template;`).

Report only real defects. Output: findings most severe first (file:line, defect, concrete failure
scenario, CONFIRMED by reading / PLAUSIBLE, smallest fix), then claims found sound, then areas not
covered.
