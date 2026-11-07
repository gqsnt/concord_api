# Adoption lane: Concord's new `query(..)` placement in two SDKs

You work in a git worktree of the Concord Rust workspace (your working directory, branch
`sdk50/b12-pairs-adopt`). Read `AGENTS.md`/`CLAUDE.md` if present, then `docs/AUTHOR_GUIDE.md`
§4.1a ("Values that choose their own keys: `query(..)`") and the `QueryPairs` rustdoc
(`crates/concord-core/src/value.rs`). You cannot commit; leave the change in the working tree.
Network is enabled for loopback tests only; contact no external service. Do not edit `crates/`.

## Question

Does `query(..) x?: T` with `T: concord::QueryPairs` remove real SDK code, or does it only move
it? Measure; do not assume. A negative or mixed result is a valid outcome — report it plainly.

## Witnesses

1. `examples/discord-rest-sdk`: three optional query values `around`/`before`/`after` declared in
   a private `mod raw` with `anchor()`/`maybe_anchor()` wrappers (src/api.rs) so callers set at
   most one. Try declaring the `Anchor` enum (src/model.rs) as a `query(..)` value instead and
   delete what becomes unnecessary (the raw module, wrappers, docs about raw setters).
2. `examples/clickhouse-sdk`: run-time `param_<name>` pairs appended after `prepare()` through
   native URL access (src/client.rs ~363, src/wire.rs header comment, src/query.rs ~438). Try
   placing the query's bindings as a `query(..)` value on the endpoints that take them.

## Rules

- Every existing test assertion stays unchanged; do not delete or weaken tests. If a public SDK
  method name changes (for example a generated `set_anchor` replacing a hand-written `anchor`),
  keep the old name working when tests or README examples use it, and say what that costs.
- Keep each SDK's documented behavior (wire order of query pairs, redaction, error kinds).
  If the new placement changes wire order or Debug output, say exactly how and whether a test
  observes it.
- Bounded verification: `CARGO_BUILD_JOBS=4`, one cargo job at a time, use the existing target
  dir; run `cargo test -p <each changed package>`, `cargo clippy -p <pkg> --all-targets -- -D
  warnings`, `cargo fmt --all -- --check`.

## Report

Per SDK: production lines removed/added (excluding tests), test/doc lines, what was deleted,
what had to stay and why, any Concord friction (a diagnostic, a missing impl, a rule that got in
the way), and whether you would recommend the placement to the next SDK author. Then the
verification commands and their results.
