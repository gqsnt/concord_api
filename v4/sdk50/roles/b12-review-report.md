Reviewed `8dc53558..d3808b29` read-only. No builds, test execution, coordinator logs, or external services were used.

1. **P1 — Refusal-mode admission can race with removal and poison the limiter.** [crates/concord-ratelimit/src/lib.rs:286](crates/concord-ratelimit/src/lib.rs:286)  
   `Mode::Refuse` releases the lock after ensuring its keys exist, without registering anything that prevents removal. Another thread can remove an inactive key before line 316’s `expect("queued keys retained")`, causing a panic while holding the mutex; subsequent operations, including lease drops, then panic on poisoning.  
   **Confidence:** CONFIRMED by reading. **Smallest fix:** keep refusal-mode ensure/check/charge under one lock, or retain keys explicitly throughout admission.

2. **P1 — Sharing one FIFO between admission and leases permits permanent deadlock.** [crates/concord-ratelimit/src/lib.rs:306](crates/concord-ratelimit/src/lib.rs:306)  
   With concurrency `1`, task A holds the lease; task B queues for that lease; A then awaits `acquire(..., Mode::Wait)`. B cannot obtain A’s lease, while A’s otherwise available admission cannot pass B in the queue. No timer resolves this cycle.  
   **Confidence:** CONFIRMED by reading. **Smallest fix:** separate admission and lease queues so rate admission can complete while its caller holds a lease.

3. **P1 — Forwarded expression fragments can change a declared limit’s arithmetic.** [crates/concord-macros/src/syntax.rs:1043](crates/concord-macros/src/syntax.rs:1043)  
   Unlike parameter initializers, `limit` uses plain `syn::Expr` parsing and skips the existing `parse_initializer`/`regroup` protection. A wrapper emitting `limit 16 / $d;`, called with `$d:expr = 1 + 3`, can round-trip as `16 / 1 + 3`: bound `19` instead of `4`, allowing a five-byte body that should fail. The local Syn printer recreates invisible groups—the case the existing regrouping code addresses.  
   **Confidence:** PLAUSIBLE; compiler behavior was not executed. **Smallest fix:** parse the bound through `parse_initializer`, and add a forwarded-expression regression asserting `decoder.limit()`.

4. **P2 — The documented composition can dispatch during a newly observed cooldown.** [crates/concord-ratelimit/README.md:47](crates/concord-ratelimit/README.md:47)  
   The recipe charges admission before waiting for concurrency. If B passes `acquire` while A holds the lease, then A observes a per-key cooldown and releases its lease, B obtains the lease and sends during that cooldown: lease admission deliberately skips per-key rate state. Long lease waits similarly make rate charges stale.  
   **Confidence:** CONFIRMED by reading. **Smallest fix:** document acquiring concurrency first through an independent limiter/semaphore, then rate admission immediately before dispatch; account for finding 2 before recommending that order on one limiter.

5. **P2 — Generated limit checking captures caller names.** [crates/concord-macros/src/emit.rs:2610](crates/concord-macros/src/emit.rs:2610)  
   The generated `const __LIMIT: usize = #limit` uses caller-context names: an outer `const __LIMIT: usize = 32;` followed by `limit __LIMIT;` becomes a self-referential constant, while a caller alias named `usize` changes the generated annotation. Valid declarations consequently fail compilation.  
   **Confidence:** CONFIRMED by reading. **Smallest fix:** pass the expression directly to `.with_limit(#limit)` inside the existing static initializer, which already enforces const evaluation and the required type.

6. **P3 — The runtime-key encoding test does not exercise runtime-key encoding.** [checks/core-concepts/tests/f3_query_pairs.rs:127](checks/core-concepts/tests/f3_query_pairs.rs:127)  
   Despite its key-and-value encoding claim, the test writes only safe keys (`param_x`, `param_name`) and checks reserved characters solely in a value. A regression leaving runtime keys unencoded would pass, although a key such as `param_x&y=z` would split into additional parameters.  
   **Confidence:** CONFIRMED by reading. **Smallest fix:** include reserved characters in a runtime key and assert both its raw encoding and decoded identity. The implementation currently uses `append_pair` correctly.

7. **P3 — Notion’s boundary documentation still assigns success limits only to domain methods.** [examples/notion-sdk/README.md:195](examples/notion-sdk/README.md:195)  
   The README says generated sends use ordinary decoding while SDK body limits apply to `NotionClient` methods; the new client-level declaration also bounds generated and prepared sends. An oversized successful page now returns `Limit` through those paths too.  
   **Confidence:** CONFIRMED by reading. **Smallest fix:** distinguish the shared declared success-body bound from domain-only error interpretation and envelope checks.

The following checked claims looked sound:

- Once constructed correctly, limits reach `Decoder::decode`/`finish`, both ordinary `Received` decode variants, and prepared sends. Drains and streams bypass finite buffering; explicit limited calls replace the declaration.
- No redaction leak or unintended over-redaction was found in the inspected query-pairs, endpoint Debug, Prepared Debug, and core Error paths. Query pairs follow declared query contributions; empty keys produce value-free Build errors.
- No defect was found in `single`/`unsigned` or their NASA/GitHub/GitLab numeric-header adoption.
- No defect was found in the media checker, charset handling, offset helper, or inspected FRED/USGS projections.
- The SDK limit migrations preserve the former success caps. Discord’s overlap change retains lower remaining counts and later resets after pruning expired windows.
- The query-credential transport example and Error-accessor disclosure match the inspected implementation.

Coverage limits: I did not exhaustively cover unchanged SDK workflows, every cfg/raw-identifier/group combination, arbitrary panicking user implementations, or provider conformance. Compiler diagnostics, feature/package checks, and runtime interleavings were assessed from source only. The stream-limit UI fixture contains only a drain case; the stream rejection branch was checked by reading.