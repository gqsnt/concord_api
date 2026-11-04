# Extension decisions

Short records of each deviation from, or refinement of, `API_PLAN.md`, with its reason.
Implementers append entries under their own prefix.

## Pagination (`crates/concord-pagination`)

- **PAG-01 — Budgets are mandatory everywhere.** `sequential(from, budget, fetch)` as planned;
  the independent `Options` and the job side of `JobOptions` are built with `Options::new(Budget)`,
  and `JobOptions::new(discovery: Budget, jobs: Options)`. `Budget` wraps `NonZeroUsize`. Reason:
  contract §3 (a page is charged at admission; a huge advertised total or an endless lazy plan must
  not dispatch unboundedly; a repeating manifest chain must end). A budget is a count of logical
  factory calls, not HTTP attempts (documented).

- **PAG-02 — `first_page_then`'s plan returns the page error type, not a crate `PlanError`.**
  Signature: `plan: FnOnce(&R) -> Result<I, E>` with `I: IntoIterator<Item = K>`. Reason: the
  adapter owns envelope validation and its typed protocol error (P05: an inconsistent envelope is a
  typed page failure, not an empty page); a crate `PlanError` would stringify or erase it. A refused
  plan turns the first page into a failure (`Keyed<K, E>`, not emitted as a success), leaves no
  remaining plan, and ends with `Outcome::DiscoveryFailed`, exactly like a failed first page.

- **PAG-03 — `manifest_jobs(from, options, discover, job)` and `Manifest = Page`.** Added a
  `from: Resume<MK>` start so discovery is resumable from `DiscoveryReport::resume`. `Manifest<D,
  MK>` is a type alias of `Page<D, MK>` (descriptors plus the next manifest key), avoiding a second
  page type. Remaining descriptors from a report are rerun with `independent` (documented on
  `JobsReport::into_rerun`) rather than a separate injection API.

- **PAG-04 — Outcome shapes returned by `next`.** `Sequential::next -> Option<Result<Page<T, K, M>,
  E>>`; `Items::next -> Option<Result<T, E>>`; `Independent::next -> Option<Result<Keyed<K, R>,
  Keyed<K, E>>>`; `Jobs::next -> Option<Result<Keyed<D, R>, JobsError<D, EJ, ED>>>` with
  `JobsError::{Job(Keyed<D, EJ>), Discovery(ED)}`. `Keyed { index, key, value }` carries the plan
  (or discovery) position so ordered combination and sparse progress do not depend on key order.
  Errors are moved to the caller by `next`; reports keep the failed *identity* (keys), plus the
  errors that were never emitted (`unemitted_errors`). Nothing is stringified.

- **PAG-05 — One shared `Outcome` enum.** `Exhausted`, `ExhaustedWithFailures`, `Bounded`,
  `Failed`, `FailureLimit`, `Stopped`, `DiscoveryFailed`, `DiscoveryBounded` (`#[non_exhaustive]`);
  each report documents which it can produce. Reason: the contract's required meanings (provider
  exhaustion, configured bound, observed failure, user interruption, incomplete discovery) with
  "a successful subset is never a complete success" made explicit by `ExhaustedWithFailures` and
  `is_success()`.

- **PAG-06 — Failure history is library-owned and bounded.** A traversal retains the key of every
  failed page/job until it ends (successes are not remembered). Bound: one key with `Stop`; with
  `Continue`, `Options::failure_limit(n)` (the failure reaching the limit stops admission with
  `Outcome::FailureLimit`), otherwise the budget. Reason: the contract requires the resume report to
  name failed pages independently of the emission cursor; a caller-owned-only ledger could not.
  Covered by the long alternating success/failure test.

- **PAG-07 — Restart description for independent work.** `Checkpoint { admitted, unfinished }`
  (snapshot) and reports list, in plan order, every admitted key without a successful emitted
  outcome with its state (`Unfinished::{InFlight, Retained, Failed, Cancelled}`), plus the
  unadmitted `rest` of the plan (or `queued` descriptors and `discovery.resume` for jobs).
  `restart()` is the first unfinished key (the ordered restart point); `failed()` lists failures
  separately; `retained` hands back completed-but-unemitted results so finished sink work can be
  reused; `into_rerun()` gives the keys still to run.

- **PAG-08 — Discovery concurrency and backlog.** One manifest call at a time in its own slot beside
  the job window (`window + 1` owned futures at most). A manifest call is admitted only while queued
  descriptors are below `JobOptions::backlog` (default: the job window) and do not already cover the
  remaining job budget; a manifest's descriptors are accepted whole, so the backlog never exceeds
  `backlog - 1 +` one manifest. Reason: smallest overlap that keeps all three quantities bounded and
  separately testable.

- **PAG-09 — Discovery-failure policy.** A manifest failure ends discovery (no guessed next key)
  and is returned as `JobsError::Discovery`. With `OnError::Stop` the jobs in flight are dropped
  (cancelled); with `OnError::Continue` the admitted *and* queued known jobs still run to
  completion. Outcome `DiscoveryFailed` in both cases; the report separates attempted, queued and
  undiscovered work. Tested both ways.

- **PAG-10 — Item checkpoints replay the page.** `ItemCheckpoint { page: Resume<K>, skip }` and
  `resume_items(at, budget, fetch)`. `Items::finish` also returns the unemitted `remainder` and
  `after_remainder` (where to continue if the caller processes the remainder itself). The page
  checkpoint advances only after a page's last item is emitted; an empty page advances it in one
  explicit transition. A stable page is caller-enforced (not detectable in general); a replayed
  page shorter than `skip` is skipped entirely (documented and tested).

- **PAG-11 — Admission happens only while `next` is polled.** A freed slot is refilled on the next
  poll, not before returning a result. Futures are lazy and polled on the caller's task, so early
  admission would gain nothing while the caller processes a result, and a stopped consumer never
  causes admission. Construction calls nothing.

- **PAG-12 — After a halt nothing more is emitted.** Once a stop-mode failure, the failure limit or
  `finish` halts the window, retained results are not emitted (they go to the report's
  `retained`); `next` returns `None`. By construction a contiguous retained prefix is always
  emitted before the scheduler polls for more completions.

- **PAG-13 — Keys are `Clone`; one box per future.** `K`/`D`/`MK: Clone` (copies kept for failed
  and restart identity); items, results and errors are moved, never cloned; no `Send`/`'static`.
  Each admitted future is boxed once (`Pin<Box<Fut>>`) so it can be tagged with its index without
  pin projection under `unsafe_code = "forbid"`; concurrent polling uses `futures-util`'s
  `FuturesUnordered` (feature `alloc` only, `default-features = false`).

- **PAG-14 — Not provided, deliberately.** No `Stream` impls (the plan's `async fn next` only), no
  completion order or continuation after errors for dependent chains (not required; a chain cannot
  continue without a next key), no raw-body lease API (the full-job facade is the contract's
  required form), no byte/record limits (the reader's, at the codec boundary).

- **PAG-15 — No core change.** The traversals need nothing from `concord-core`: SDK adapters use
  generated operations and typed responses through closures.
