Reviewed `d3808b29..78b1ff77` and the prior seven findings. No builds, test execution, writes, or external calls.

1. **P2 — The caller-name capture fix is incomplete.** [emit.rs:2635](crates/concord-macros/src/emit.rs:2635)  
   With a caller declaration `const __DECODE: usize = 32;` and `limit __DECODE;`, the expression lands inside the initializer of generated `static __DECODE: DecodeFormat<T>`. It resolves to that static instead of the caller’s constant, causing a type error. `__META` has the same problem. This is a residual defect in the earlier hygiene finding; the new `__LIMIT` regression does not cover it.  
   **Confidence:** CONFIRMED by reading.  
   **Smallest fix:** Make generated static names collision-safe, or evaluate the bound outside their scopes. Add regressions for `__DECODE` and `__META`.

2. **P3 — “Before block entries” overstates the query-order guarantee.** [AUTHOR_GUIDE.md:447](docs/AUTHOR_GUIDE.md:447)  
   Suppose the client declares `"v" = "1"`, an endpoint’s pairs value writes `p=9`, and its block contains `append "v" = "2"`. The generated query is `v=1&v=2&p=9`: the entire inherited slot is emitted before the pairs value. Block overrides and re-placements of inherited keys likewise retain the original slot position. This follows §4.7, but contradicts the unqualified statement that pairs precede endpoint block entries. The same wording appears in `value.rs`.  
   **Confidence:** CONFIRMED by reading.  
   **Smallest fix:** Say “before keys first introduced by the endpoint’s block,” explicitly preserving existing-slot ordering; add an inherited-append witness.

3. **P3 — The cancellation regression can miss an erroneous rate charge.** [regressions.rs:376](crates/concord-ratelimit/tests/regressions.rs:376)  
   The test uses a 10 ms budget window, cancels the waiter at time zero, then advances to 100 ms before checking that admission remains available. An incorrect charge made while queued or during cancellation would already have expired, so those assertions still pass. The lease assertion does detect an extra lease; the rate assertion does not establish its stated guarantee.  
   **Confidence:** CONFIRMED by reading; no production cancellation leak established.  
   **Smallest fix:** Use a budget window longer than the cooldown, or test cancellation with another budgeted key while its window remains active.

Checked claims found sound:

- New-key ordering handles group splices, absent pairs, pair-only endpoints, and endpoints with only block entries. Groups expand before indexing. Per-value `#[cfg]` is rejected; owner gating covers generated items. Removed/re-placed inherited keys retain their documented first-appearance position.
- The forwarded-expression grouping fix, removal of `const __LIMIT: usize`, and primitive-alias regression address their specific earlier failures. The revised encoding test actually exercises reserved characters in a key.
- Discord retains exclusive cursor replacement/clearing and cursor-before-limit wire order. ClickHouse retains validation, sorted bindings, and the same execution boundary. Existing SDK assertions were not weakened.
- Credential redaction remains intact in the paths read. Discord endpoint `Debug` now additionally withholds cursor IDs; that is an observable redaction change, explicitly asserted by the new test.
- Rate-limit setup and first check share one lock; both queues prevent removal while waiting. Separate queues close the reported lease-holder deadlock. Poison recovery and lease-before-admission composition address the earlier failures. No additional ordinary cancellation leak or deadlock was established.
- The Clone explanation matches endpoint-owned value/body bounds. The FRED excerpt matches its implementation; the dated ClickHouse note and Notion bound correction match current code.

Not covered: compilation, runtime interleavings/model checking, rendered rustdoc, the full feature/platform matrix, arbitrary custom `Hash`/`Clock`/`Waker` panic behavior, or unchanged SDK internals beyond the affected execution/redaction paths. Campaign verification counts and role-provenance claims were not independently validated.