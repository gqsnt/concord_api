# Independent follow-up review: Concord between-waves changes after `d3808b29`

You are an independent reviewer (read-only). The working directory is a git worktree of the
Concord Rust workspace on branch `sdk50/integration`. Review `git diff d3808b29..HEAD` (see
`git log --oneline d3808b29..HEAD`). An earlier review covered `8dc53558..d3808b29`; its seven
findings and the responses are in `v4/sdk50/waves/W1/SYNTHESIS.md` ("Independent review").
Do not build (read-only sandbox); read code and tests. Contact no external service.

## What changed (claims to check, not to trust)

1. `query(..)` values are written at their declared position among an endpoint's parameters
   (after inherited keys, before `query { }` block entries) — `Contrib::param` in
   crates/concord-macros/src/model.rs and the ranking loop in emit.rs `prepare_body`.
2. Discord (`examples/discord-rest-sdk`) replaced its private `raw` module and wrapper layers with
   `query(..) anchor?: Anchor`; ClickHouse (`examples/clickhouse-sdk`) replaced post-prepare URL
   mutation with `query(..) params?: Vec<(String, String)>`. Claimed: existing test assertions,
   wire order and SDK-level redaction unchanged.
3. Fixes for the earlier review: `limit` parsed through `parse_initializer`; the bound passed to
   `with_limit` as written (no generated `const __LIMIT: usize`); tests in
   checks/core-concepts/tests/b12_declared_limit.rs and f3_query_pairs.rs.
4. `concord-ratelimit`: setup and first check under one lock, poisoning recovered, independent
   admission and lease FIFO queues, README composition order (lease, then admission right before
   dispatch); regression tests in crates/concord-ratelimit/tests/regressions.rs.
5. Generated rustdoc note on the endpoint `Clone` impl; docs (CUSTOM_FORMATS §7 excerpt, SDK20
   READOUT dated note, Notion README).

## Look for

Correctness and safety defects with a concrete failure scenario: a `query(..)` position that is
wrong for groups (`..Name` splices), cfg-gated values, `remove` + re-placement of an inherited
key, or endpoints with only block entries; a redaction change; a ratelimit interleaving that still
panics, deadlocks, leaks a charge/lease on cancellation, starves one queue, or violates the
documented order; a test that would pass without the fix it claims to cover; Discord/ClickHouse
behavior changes not covered by tests; doc claims the code does not satisfy.

## Output

Findings most severe first: file:line, defect, concrete failure scenario, confidence (CONFIRMED
by reading / PLAUSIBLE), smallest fix. Then checked claims you found sound, and areas you did not
cover. A missing finding is not approval of an area you did not read.
