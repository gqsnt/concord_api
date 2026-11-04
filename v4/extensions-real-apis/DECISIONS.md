# Extension decisions

Short records of each deviation from, or refinement of, [API_PLAN.md](API_PLAN.md), with its
reason. Implementers append entries under their own prefix (`PAG-`, `CSV-`, `JSONL-`, `SDK-`,
`COORD-`).

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

## JSON Lines / consecutive JSON values (`concord_extras::jsonl`)

**JSONL-01 — `MediaPolicy` lives in `jsonl`.** The plan names `media(MediaPolicy)` for both CSV and
JSONL options without giving the type a home. It is defined in `concord_extras::jsonl` so the
parallel CSV work does not conflict in `stream`; the coordinator may hoist it to `stream` if CSV
adopts the same shape. Shape: a `Copy` value with `const` constructors `standard()`,
`standard_or(&'static [&'static str])`, `only(&'static [&'static str])` and `any()`. Entries are
validated as `type/subtype` RFC 9110 tokens (no parameters, no `*`) like
`HeaderValue::from_static`: a compile error in a `const`, a panic otherwise. This keeps
`JsonLinesReader::new` infallible, as planned, with no invalid configuration left to report.

**JSONL-02 — Documented media types.** `JsonLines`: `application/jsonl`, `application/jsonlines`,
`application/x-jsonlines`. `Ndjson`: `application/x-ndjson`, `application/ndjson`. The families do
not accept each other (a provider's actual type is accepted deliberately with `MediaPolicy`).
`JsonValuesReader`: `application/json` and `application/*+json`, as `Json`; parameters such as
Kubernetes' `stream=watch` are ignored, `application/vnd.kubernetes.protobuf;stream=watch` is
refused. All non-`any` policies require exactly one well-formed `Content-Type` (not a media range)
whose charset, if any, is `utf-8`/`us-ascii` declared once; a missing field is refused. Request
`Content-Type` and `Accept` suggestion: `application/jsonl` / `application/x-ndjson`.

**JSONL-03 — Finite `Decode` = preset, exactly.** `Decode<Vec<T>>` for `JsonLines`/`Ndjson` runs the
reader's framing code over the collected body with `JsonLinesOptions::json_lines()`/`ndjson()`,
including the default 16 MiB line limit (`DEFAULT_MAX_RECORD_BYTES`, also the default
`max_value_bytes`). One rule, one implementation; a caller needing another limit on a collected
body reads it with a configured `JsonLinesReader` over `Chunks`.

**JSONL-04 — What `RecordErrors::Yield` recovers.** Recoverable, reported as an `Err` item for one
line delimited on both sides (LF or the true end): a refused blank line, invalid UTF-8, invalid
JSON (including a truncated final line) and a `Decode` mismatch. Always terminal: a line over
`max_record_bytes` (the reader stops before the line's end, so it is not isolated and is not
skipped), a refused byte order mark, transport errors and totals. `JsonValues` has no yield mode:
every error is terminal.

**JSONL-05 — UTF-8 validated per line before serde.** serde_json does not validate the bytes of a
string it skips (`IgnoredAny`, unknown fields), so the profile's UTF-8 rule is enforced with
`std::str::from_utf8` on every line/value, whatever `T` reads. Cost: one extra linear pass per
record.

**JSONL-06 — Limits count content.** `max_record_bytes` counts a line's bytes excluding its LF or
CRLF; the line buffer may hold one extra pending CR (it may start a CRLF), so its length and
allocation stay within `max + 1`. A trailing CR at the true end (no LF) is content.
`max_value_bytes` counts a value from its first to its last byte, whitespace excluded; the buffer
stays within `max`. Both are enforced before the buffer grows; a line/value wholly inside one chunk
is checked the same way (chunk-independent outcome). The optional byte total is the foundation's:
checked per received chunk.

**JSONL-07 — Writer.** Compact serde_json, LF after every record including the last, no BOM; the
same bytes for both families. A record whose JSON contains an LF (possible only through
`serde_json::value::RawValue`) is refused, since it would split the record. `JsonLinesWriter` uses
only the options' family (its `Content-Type`); reader-only options do not apply. Added
`JsonLinesWriter::content_type()`; `to_vec` takes any `IntoIterator<Item = T: Serialize>`.

**JSONL-08 — Consecutive-value boundaries.** A number/literal ends at JSON whitespace, at the start
of `{`, `[` or `"`, or at the true end of input — serde_json `StreamDeserializer`'s documented rule
(`12{"a":1}` is two values, `truefalse` is one malformed token). `,`, `:`, `}` and `]` outside a
container belong to the token and fail when it is parsed (`Format`); so does a byte order mark.
Input ending inside an object, array or string is `Format` ("the input ends inside a JSON value");
a literal cut at the true end is `Format` ("a JSON value is incomplete").

**JSONL-09 — Work per poll.** `next()` yields to the executor once after consuming `SKIP_BUDGET`
(64 KiB) without producing a record — counting every consumed byte, not only skipped ones — so an
always-ready source cannot make one poll unbounded. The value scanner also stops after
`SKIP_BUDGET` whitespace bytes inside one chunk. Scanning one line/value is bounded by
min(chunk, limit).

**JSONL-10 — Resource accessors (coordinator request).** `JsonLinesRecords` and `JsonValues` have
`retained_bytes()` (length of the library-owned buffer holding a line/value that started in an
earlier chunk; `0` between records; excludes the current chunk's unconsumed remainder and returned
records), `peak_retained_bytes()` (its maximum over the reader's life, including a complete line
while parsed from the buffer), `pending_input_bytes()` (the current chunk's unconsumed remainder,
the source's memory, shared not copied) and `into_source()` (documented as partially consumed,
never unread). A buffered line is released right after it is parsed.

**JSONL-11 — Foundation touch points.** `media.rs` only: the dead-code allowance of `Media::params`
now applies whenever `csv` is off (it is read only by CSV), and `check_charset`/`essence` are
compiled for `jsonl` too. With `--all-features` the `params` field is still unread while `csv` is a
stub on this branch (the same warning exists before this change); it disappears once CSV reads it.
No change to `Driver`, `ReadError`, `OpenError` or `RecordBody`.

**JSONL-12 — Test-only additions.** A dev-dependency entry for the existing `serde_json` with its
`raw_value` feature, to test the writer's LF guard (no new crate). The seed directories carry
`.gitattributes` `* -text`: the repository's `* text=auto` would otherwise normalize the CRLF seed.

**JSONL-13 — Observed upload failure timing (hyper 1.x through reqwest 0.13, loopback).** A source
or serialization failure in the body's first poll fails `send` before anything reaches the
receiver (it observed 0 bytes, not even the head: hyper writes the head with the first body
frame). A failure after a frame was handed over leaves the receiver with the complete records of
the earlier frames and no terminating chunk; `send` returns a reqwest error (`is_request`) whose
source chain contains the `UploadError` (label, records serialized). Nothing is replayed.
