# Fix lane: three review findings in `concord-ratelimit`

You work in a git worktree of the Concord Rust workspace (your working directory, branch
`sdk50/b12-ratefix`). Read `AGENTS.md`/`CLAUDE.md` if present and `crates/concord-ratelimit`
(src/lib.rs, README.md, tests). You cannot commit; leave the change in the working tree. Network
is enabled for loopback tests only; contact no external service. Edit only
`crates/concord-ratelimit` (and `examples/native` / `examples/riot` only if an API change forces
it; their test assertions must stay unchanged).

An independent read-only review reported these defects (its text, to verify — not to trust):

1. **Refusal-mode admission can race with removal and poison the limiter** (src/lib.rs ~286):
   `Mode::Refuse` releases the lock after ensuring its keys exist, without registering anything
   that prevents removal. Another thread can `remove` an inactive key before the
   `expect("queued keys retained")` (~316), panicking while holding the mutex; later operations,
   including lease drops, then panic on poisoning.
2. **One FIFO shared by admission and leases permits permanent deadlock** (~306): with
   concurrency 1, task A holds the lease; task B queues for that lease; A then awaits
   `acquire(.., Mode::Wait)`. B cannot obtain A's lease, and A's available admission cannot pass
   B in the queue. No timer resolves the cycle.
3. **The documented composition can dispatch during a newly observed cooldown** (README ~47):
   the recipe charges admission before waiting for concurrency. If B passes `acquire` while A
   holds the lease, then A observes a per-key cooldown and releases, B obtains the lease and
   sends during that cooldown (lease admission skips per-key rate state). Long lease waits make
   rate charges stale.

## What to do

For each: first write a deterministic test that reproduces it on the current code (a real
multi-threaded executor where interleaving matters; injected clock; bounded with a timeout so a
deadlock fails instead of hanging) and confirm it fails. Then make the smallest fix and show the
test passing. If you find a finding is not reproducible, say so with the attempted test and keep
that test only if it documents a real guarantee. Do not weaken or delete existing tests.

Also: no `unwrap()`/`expect()` on the limiter's mutex or map lookups may panic on a state that
another public call can produce; poisoning should not cascade into `Drop` (a lease drop must not
panic). State the fairness/ordering rule that results from separating queues, in rustdoc and
README. Fix the README composition so the documented order is correct (for example: concurrency
first, rate admission immediately before dispatch), and say why.

Verification (`CARGO_BUILD_JOBS=4`, one cargo job at a time): `cargo test -p concord-ratelimit
--all-features`, `cargo test -p concord-ratelimit --no-default-features`, `cargo test -p
concord-example-native`, `cargo test -p concord-example-riot` (use the package names from their
Cargo.toml), clippy `--all-targets --all-features -D warnings` for those packages, rustdoc with
`-D warnings`, `cargo fmt --all -- --check`.

## Report

Per finding: reproduced or not (test name, failure output before), the fix, the test after.
Public API changes (one line each with justification). Test counts. Anything left undone.
