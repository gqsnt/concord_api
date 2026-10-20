# V3-O07 steady head-cost attribution (spec §28.4)

This file attributes, allocation by allocation, the v3 versus v2.1 deltas that
`head_cost_linux_2026-09-26.txt` reported for request construction, binding,
bound-head planning and managed execution, classifies each one as (a) newly
required functionality (with the spec rule) or (b) an avoidable regression in
an unchanged hot path, and records what was corrected, what remains and why.
The final measurements are `head_cost_linux_2026-09-26_final.txt` and
`expansion_scale_linux_2026-09-26_final.txt`; the earlier files are kept as
history.

Trees compared (same host, toolchain and declarations):

| label | source |
|---|---|
| v2.1 | `git archive baseline-v2.1` = eefca26 (unmodified v2.1 export), extracted to a scratch directory |
| v3 before | 0c7b243, the base of this slice, from an archive export to a scratch directory (same allocation counts as the earlier file's e6d702a) |
| v3 final | d30e23d, after the corrections below (this workspace) |

The scratch harnesses were built in a separate target directory with
`--release` and `debug = "limited"`; the three trees link the same external
dependency versions (each tree's own `Cargo.lock`).

## Method

- **Allocation attribution.** A scratch harness (not in the repository; it
  needs `unsafe` for a `GlobalAlloc`) includes the exact `head_cost`
  declarations (`src/head_cost/declaration.rs`, `declaration_v2_1.rs.txt`) and
  installs a tracking global allocator that records a backtrace for every
  allocation on the current thread during exactly one operation, after the
  same 1000 warm-up operations and with inputs prepared outside the tracked
  region. `realloc` counts as an allocation, as in `allocation-counter`. Built
  `--release` with line tables, it reproduces the probe's allocation counts
  and bytes exactly for every row in all three trees, so the traces account
  for every allocation the probe counts. A second harness does the same for
  one steady-state loopback execution.
- **Instructions.** `valgrind --tool=callgrind --toggle-collect` around a loop
  of 2000 operations (after 1000 warm-up operations), system allocator; the
  per-operation count is identical for 2000 and 4000 operations. Repeated runs
  of an unchanged binary differ by at most about 12 instructions per plan.
- **Atomic reference counting.** callgrind `--dump-instr=yes`, summing the
  executed count of every `lock`-prefixed or `xchg` instruction (addresses from
  `objdump -d`). The absolute numbers count instructions, not logical clone or
  drop operations; only differences between trees are used.
- **Microarchitecture.** callgrind `--cache-sim=yes --branch-sim=yes`. This is
  a simple simulator; its branch and L1i figures moved between two v3 builds
  whose planning code was identical (search: 100 then 75 simulated indirect
  mispredictions per plan), so they are layout-sensitive and are used only as
  supporting evidence. No hardware performance counters are available on this
  host (`perf` is not installed).
- **Time.** The `head_cost` probe, unchanged, with the earlier file's protocol
  (six alternating runs per version).

## Summary per row

Allocations and bytes per operation (deterministic):

| operation | v2.1 | v3 before | v3 final |
|---|---|---|---|
| direct request construction (scope + search) | 5 / 168 B | 6 / 200 B | 5 / 176 B |
| unbound endpoint construction (scope + show) | 2 / 80 B | 3 / 104 B | 2 / 80 B |
| request binding (`client.request`) | 0 / 0 B | 1 / 32 B | 0 / 0 B |
| plan: scope + endpoint path (`client.plan`, unbound) | 49 / 3402 B | 54 / 3624 B | 48 / 3378 B |
| plan: query program (search) | 82 / 6143 B | 90 / 6441 B | 81 / 6119 B |
| plan: bearer placement (protected) | 41 / 3014 B | 44 / 3140 B | 40 / 2990 B |
| plan: rate policy (limited) | 62 / 3883 B | 71 / 4598 B | 56 / 4213 B |
| managed execution, client thread | 63 / 24647 B | 65 / 26919 B | 63 / 26783 B |

callgrind instructions per operation / executed atomic instructions per
operation:

| operation | v2.1 | v3 before | v3 final | final vs v2.1 |
|---|---|---|---|---|
| direct construction | 1635 / 47.2 | 1842 / 39.4 | 1684 / 39.4 | +3.0% Ir |
| unbound construction | 801 / 9.9 | 978 / 15.8 | 778 / 9.9 | -2.9% Ir |
| binding | 566 / 17.7 | 860 / 31.5 | 540 / 13.8 | -4.6% Ir |
| plan: show | 31975 / 25.6 | 33133 / 45.3 | 32031 / 33.5 | +0.2% Ir |
| plan: search | 54057 / 25.6 | 56291 / 33.5 | 54923 / 25.6 | +1.6% Ir |
| plan: protected | 27571 / 35.5 | 27374 / 43.4 | 26574 / 35.5 | -3.6% Ir |
| plan: limited | 35525 / 27.6 | 36641 / 47.3 | 32404 / 43.4 | -8.8% Ir |

Time, median of the six per-run medians (min of all batches in brackets),
v2.1 → v3: earlier file (e6d702a) versus final file (d30e23d):

| operation | earlier | final |
|---|---|---|
| direct construction | -5.4% (-5.5%) | -9.8% (-3.6%) |
| unbound construction | +27.0% (+25.7%) | -0.5% (0.0%) |
| binding | +63.4% (+62.0%) | -9.3% (-10.5%) |
| plan: show | +11.4% (+10.8%) | +6.6% (+4.4%) |
| plan: search | +10.0% (+10.0%) | +4.3% (+4.5%) |
| plan: protected | +6.7% (+7.2%) | +3.3% (-0.8%) |
| plan: limited | +10.9% (+10.6%) | -0.8% (-1.6%) |

Per-run medians overlap between the versions in every planning row of the
final file, and an earlier block with the same protocol at f0c689e (identical
planning code; raw log not retained) put the search row at +11.6%: treat about
±5% as the resolution of the time comparison.

## Attribution table

Code sites are from the backtraces. "Fixed" means the allocation (or atomic
update) no longer exists in v3 final and the behavior is unchanged; each fix
has regression tests (see the commits).

### Construction and binding

| id | delta (v3 before vs v2.1) | cause | class | status |
|---|---|---|---|---|
| C1 | +1 alloc, 24 B, per scope selection (direct and unbound) and per scope value control | the generated scope state and unbound scope wrapped the scope's values (each already an `Arc<T>`) in one more `Arc<__Local>` | (b) | fixed bb67f19: values stored inline in `__State` and `Unbound` |
| C2 | +1 alloc, 32 B, per bound scope selection | `Arc<__State>`: the shared scope snapshot joined to its parent | (a) §4.5 typed shared snapshot storage; the same `Arc` is the §16.3 owner projection that `owner.bind(runtime_endpoint)` shares as `Arc<dyn OwnerProjection>` | remains; offset by C3 |
| C3 | -1 alloc, 24 B, per absent optional value (search's `page`) | v3 stores an optional slot as `Option<Arc<T>>`, so an absent value allocates nothing; v2.1 allocated `Arc<Option<T>>` even when absent (both share one `Arc` between an initialized value and its retained default) | representation difference | v3 advantage; direct construction: 5 = 5 allocations |
| B1 | +1 alloc, 32 B, per `client.request(intent)` | `Unbound::__bind` built `Arc<__State>` joining the client with the intent's captured scope snapshot; v2.1 flattened ancestor values into each request | (b): the join need not allocate | fixed cafe625: a scope-owned request holds its `__State` by value and binding moves the consumed snapshot into it (depth d: d-1 ancestor `Arc`s). A prototype showed no slowdown of direct construction (one more reference-count update) within run-to-run noise |
| B2 | 14 extra atomic instructions per bind | binding cloned the snapshot `Arc`s and a services `Arc` | (b) | fixed bb67f19/cafe625 (moves, core taken from the client): 17.7 (v2.1) vs 13.8 (final) |

### Bound-head planning (program and head)

| id | delta (v3 before vs v2.1) | cause | class | status |
|---|---|---|---|---|
| P1 | +1 alloc, 32 B, per `client.plan(&unbound_intent)` | the same owner join as B1, into an `Arc` the plan only borrows | (b) | fixed 08d2393: joined on the stack (`__bind_state`) |
| P2 | +1 alloc, 64-96 B, per nesting level that extends the program chain (show 1, search 2) | generated `visit_program` built a `Vec<ProgramSegment>` per level | (b) | fixed 08d2393: `visit_extended_program` composes chains in a stack array (Vec only past 16 segments); also used by runtime endpoint composition |
| P3 | +2 allocs, 64 B each, per plan with query and header operations | `select` returned one survivor `Vec` per family | (b) | fixed 08d2393: inline survivor buffer with exact `Vec` semantics (spills past 16) |
| P4 | +1 alloc per surviving query/header occurrence (show/protected/limited 2, search 5; 3-16 B each) | the sticky-sensitivity lookup normalized every survivor's key into a `String`, even with no sensitive operation | (b) | fixed 08d2393: keys normalized only when the family has a sensitive contribution |
| P5 | -1 alloc, 24 B | v2.1 parses the static response `Accept` with `mime` on every plan and execution; v3 validates it at compile time (§9.5) and at runtime with an allocation-free validator | (a) §9.5 | v3 advantage |
| A1 | about 8 atomic instructions per plan | `RequestMeta::from_description` and the description copied into the plan cloned and dropped the endpoint ID's `Arc<str>`; v2.1 request metadata used static IDs | (b) | fixed 834a5a3: descriptions borrow the ID from the `'static` contract |
| A2 | +7.9 atomic instructions per unbound plan (show row only) | joining the unbound intent's snapshot with the client clones the client `Arc` and the scope value `Arc`s into the stack projection and drops them | (a) §7.3: the selected client supplies root values and services at binding/planning; direct requests do not pay it (search, protected: equal to v2.1) | remains |

### Rate-limited head (limited row)

| id | delta | cause | class | status |
|---|---|---|---|---|
| R1 | +3 allocs (224 + 96 + 320 B) per rate-limited plan | per-plan declaration phase: the declaration's dimension and window vectors and the head's declaration list | (a) §12.1: built-in `origin`/`endpoint` dimensions are resolved from the finished head, so the logical origin includes every host label (v2.1 resolved at push time and computed the origin twice) | remains |
| R2 | +1 alloc (19 B) per plan, plus 2 more copies | `RateLimitId::new` from the literal on every plan, then copied into the resolved policy and its schema | (b) | copies fixed 08d2393 (shared); per-plan allocation fixed f0c689e (generated declarations borrow the literal via `RateLimitDeclaration::new_static`); atomics 53.2 → 43.4 per plan |
| R3 | +4 allocs (dims 40 B, label 6 B, windows 24 B, `Arc` 72 B) per policy per plan | value-free schema descriptor (ID, ordered labels, encodings, canonical windows, framing version) | (a) §12.5 (and the §18.4 plan's key-label schemas) | remains |
| R4 | +5 allocs per plan | `RatePlanSummary` deep-copied every schema (4) plus its own `Vec` | `Vec`: (a) §18.4; deep copy: (b) | deep copy fixed 08d2393 (schema shared by `Arc`); 1 remains |
| R5 | v2.1: 4 allocs, twice per plan; v3 before: 5 allocs, once | origin normalization (`to_ascii_lowercase` copies and a growing `format!`) | unchanged path | improved 08d2393 to one exactly sized buffer (1 alloc); v3 now 7 below v2.1 here |
| R6 | +15.8 atomic instructions per plan (final vs v2.1) | shared key-part values (`Arc<[u8]>`), the schema `Arc` shared by policy and plan summary, the plan `Arc` | (a) §12.5/§18.4 sharing of schema and key facts | remains; allocations are 6 below v2.1 and instructions 8.8% below |

Final limited-row composition: the program and head part equals v2.1 minus P5;
the rate part is 19 allocations (R1 3, origin 1, resolution 12 of which R3 is 4,
plan origin and plan `Arc` 2, R4 1) versus 24 in v2.1. Bytes are +330 B,
mostly the capacity-4 vectors of R1.

### Managed execution (client thread)

| id | delta | cause | class | status |
|---|---|---|---|---|
| E1 | +2 allocs (32 + 104 B) | `prepare_request_body` → `apply_body_content_type(None)` → `apply_media_headers` cloned the public header map for a body-free request although nothing was placed | (b) | fixed d30e23d: early return after the existing Content-Type conflict check |
| E2 | +1 alloc, 24 B | `StreamHandoff::pending` of the accepted-response stream context | (a) §15 selected finite lease | remains |
| E3 | same count, +1784 B and +352 B | the boxed operation future and the buffered acquisition future are larger | (a) §13 operation journal and phase authority, §15 | remains; not broken down per field |
| E4 | -1 alloc, 24 B | v2.1's runtime `Accept` parse (as P5) | (a) §9.5 | v3 advantage |

Final: 63 = 63 allocations; +2136 B.

## What remains and why

- **Planning time, show row (+6.6% median, +4.4% min).** Allocations -1 and
  instructions +0.2% versus v2.1; the extra cost identified is A2 (about eight
  atomic read-modify-writes for joining the unbound snapshot, required by §7.3
  for `client.plan(&intent)`). The remainder is not isolated.
- **Planning time, search row (+4.3% median, +4.5% min in the final file; up
  to +11.6% in an earlier block).** Allocations -1, atomics equal, instructions
  +1.6% (+866 per plan). The instruction delta is the symbolic patch-program
  evaluator of PATCH-01/03 (select surviving occurrences, then encode, through
  dyn-dispatched presence and emission readers): about +2,100 instructions
  of evaluator code (`apply_head_program`, `select_into`, `OccurrenceSink`,
  emission readers, chain composition) against about -670 of v2.1's direct
  emission calls and -665 from the cheaper `Accept` validation. The
  time beyond the instruction delta is attributed by callgrind's simulation to
  more L1i misses (604 → 693 per plan) and indirect-branch mispredictions
  (49 → 75), consistent with dyn dispatch and a larger code footprint, but the
  simulator's figures are layout-sensitive and there are no hardware counters
  on this host, so this residual is **partly unexplained**. One experiment
  (selecting both families' survivors in one pass) did not change the measured
  time and was reverted. Possible follow-ups, not done here: record
  always-present emissions in the static `PatchOp` table so selection skips
  the dyn presence call, or emit specialized straight-line emission code for
  fully static programs (§16.3 allows either).
- **Rate-limited planning.** R1, R3, R4 (the `Vec`) and R6 are required by
  §12.1, §12.5 and §18.4; the row is now at parity in time (-0.8% median) with
  six fewer allocations than v2.1.
- **First plan per process.** In v3 the first plan of an endpoint in a fresh
  process also builds the lazily allocated static contract description
  (§18.1 allows static metadata to be lazily allocated once); a scratch check
  measured the first `client.plan(&unbound)` at 124-153 µs (v3) versus
  24-30 µs (v2.1), with later plans equal. This is why the existing
  `concord_overhead` bench, a cold 100-iteration batch, reports "safe public
  planning" at about 306-313 µs (v3) versus 182-186 µs (v2.1).
- **Compile time and size** (expansion_scale): +34% build at 1000 endpoints
  and +28.6% binary, from generated code for new functionality (typed owner
  projections and value readers, per-endpoint request/intent/view types, the
  value-control matrix, the full static contract description). Not broken
  down per generated item in this slice.

## Corrections (commits)

| commit | change | effect |
|---|---|---|
| 08d2393 | allocation-free patch-program selection (inline survivors, keys normalized only when sensitive), stack program chains, stack owner join for unbound plans, shared rate schemas and IDs, one-buffer origin normalization | show 54 → 48, search 90 → 81, protected 44 → 40, limited 71 → 57 allocations |
| bb67f19 | scope values stored inline; binding moves the consumed snapshot | direct 6 → 5, unbound 3 → 2 allocations |
| cafe625 | scope-owned requests hold their owner state by value | binding 1 → 0 allocations; 99-116 → 71-82 ns in a prototype comparison |
| 834a5a3 | generated descriptions borrow the static contract ID | about 8 fewer atomic instructions per plan (search, protected now equal to v2.1) |
| f0c689e | generated rate declarations borrow their literal ID | limited 57 → 56 allocations, atomics 53.2 → 43.4 |
| d30e23d | no header-map copy for absent body media | execution 65 → 63 allocations |

Regression tests: `request::program` tests (survivor spill past the inline
capacity, inline/heap chain composition), `tools/perf/tests/owner_snapshots.rs`
(§4.5 snapshot law across clones, earlier intents and nested scopes;
direct/bound/planned equivalence; the corrected allocation counts),
`inspection::tests::endpoint_description_derives_runtime_facts_from_contract`
(static ID), `rate_limit::types::tests::static_and_shared_rate_limit_ids_are_one_identity`,
`request::head::tests::absent_media_headers_leave_the_head_unchanged`.

## r2 remeasurement (repaired tree, V3-O07; external review §6)

Measured at **89b2cbc** (clean): the repaired tree ed1ef3c (review repairs
F01-F03 merged) plus two commits that only add the first-use probe
`tools/perf/src/bin/head_first_use.rs`. Files:
`head_cost_linux_2026-09-26_r2.txt` (warm), `first_use_linux_2026-09-26_r2.txt`
(fresh process, no warm-up), `expansion_scale_linux_2026-09-26_r2.txt`
(scale). Nothing under `crates/` was changed in r2; everything below is
measurement and attribution. "Bytes" are allocated-byte counters (allocation
traffic), not peak resident memory or retained heap.

### Method (r2)

- **Allocation attribution.** The scratch tracking-allocator harness of the
  first round, rebuilt (not in the repository: it needs `unsafe` for a
  `GlobalAlloc`): it includes the exact `head_cost` declarations, records the
  size and a backtrace of every allocation and realloc on the main thread
  during exactly one operation, and was built `--release` with line tables
  against v2.1 (eefca26), v3 89b2cbc and archive exports of intermediate v3
  commits. It reproduces the probes' counts and bytes exactly (warm
  execution 63 / 24,647 B and 63 / 27,991 B; warm plans; first plan 527 /
  71,305 B and 53 / 4,178 B; first execution 162 / 81,556 B and 94 / 65,727 B).
- **Future sizes.** `std::mem::size_of_val` in the harness, and the
  coroutine layouts from `-Zprint-type-sizes` (RUSTC_BOOTSTRAP=1) of the
  harness crate (where the generic execution futures are monomorphized) and
  of concord-core (the non-generic response futures), decomposed along the
  largest suspend state of each nested `.await`.
- **Per-commit bisection.** The same harness built against archive exports
  of d30e23d, 8eba413, a4d679a, a20a331, f44219b, 4abb2db, e1daf94 and
  f3e01e4 (first-parent merges between the earlier measurement and the
  repaired tree).
- **Instructions, atomics, cache.** `valgrind --tool=callgrind
  --collect-bus=yes --toggle-collect=*measured_loop*` around 2000/4000 plans
  (after 1000 warm-up) or 100/200 executions (after 50), with the harness's
  tracking allocator in both versions (glibc malloc underneath). "Ge"
  (global bus events) counts executed lock-prefixed/atomic instructions,
  including glibc's own. Per-operation values agree between the two loop
  lengths. `--cache-sim=yes --branch-sim=yes` for the planning rows; as in the
  first round this is a simple simulator used only as supporting evidence (no
  hardware counters on this host).

### Warm summary (16 alternating runs per version, two blocks)

| operation | allocations v2.1 → v3 | bytes/op v2.1 → v3 | time (median of medians) | Ir/op v2.1 → v3 | atomics (Ge)/op |
|---|---|---|---|---|---|
| direct construction | 5 → 5 | 168 → 176 (+8) | −10.9% | | |
| unbound construction | 2 → 2 | 80 → 80 | −7.8% | | |
| binding | 0 → 0 | 0 → 0 | −17.6% | | |
| plan: show | 49 → 48 | 3,402 → 3,378 (−24) | +6.7% | 32,698 → 32,897 (+0.6%) | 56 → 60 |
| plan: search | 82 → 81 | 6,143 → 6,119 (−24) | +5.6% | 56,054 → 56,123 (+0.1%) | 108 → 108 |
| plan: protected | 41 → 40 | 3,014 → 2,990 (−24) | +4.0% | 27,172 → 26,999 (−0.6%) | 47 → 45 |
| plan: limited | 62 → 56 | 3,883 → 4,213 (+330) | −1.0% | 36,818 → 33,467 (−9.1%) | 100.7 → 79.0 |
| managed execution | 63 → 63 | 24,647 → 27,991 (+3,344) | +4.1% | 81,493 → 84,958 (+4.3%) | 377.4 → 374.6 |

### Managed execution: +3,344 B/op at equal count

Only two of the 63 allocations differ in size (the other 61 have identical
sizes in both versions): the boxed operation future of `IntoFuture`
(`execute/mod.rs`, 4,624 → 7,616 B, +2,992) and the boxed buffered-acquisition
future (`response/spec.rs` `acquire`, 2,376 → 2,728 B, +352). Both are
`size_of` of a coroutine state, so the whole delta is future size.

History of the boxed operation future (harness at each commit):

| commit | execution bytes/op | boxed future | change | cause |
|---|---|---|---|---|
| d30e23d (earlier final file) | 26,783 | 6,408 | | |
| 8eba413 (slice F merged) | 26,783 | 6,408 | 0 | |
| a4d679a (slice E, EXEC-01) | 27,031 | 6,656 | +248 | journal 176 → 216 (+40), provider link (+48), work future +160 |
| a20a331 (review F01) | 27,991 | 7,616 | +960 | forwarding wrapper level +744; finalization state +216 |
| f44219b (F02) … f3e01e4, 89b2cbc | 27,991 | 7,616 | 0 | F02 and the F01 follow-ups change no execution byte |

The review's 27,031 B is exactly a4d679a; this tree is +960 B above it.

Decomposition of the 7,616-byte v3 future versus the 4,624-byte v2.1 future
(largest suspend state at each level; "other" = everything except the awaited
inner future):

| level | v2.1 | v3 | delta | content |
|---|---|---|---|---|
| `into_future` async block | 56 | 160 | +104 | holds the `Request` (48 → 152 B) |
| `execute_request` | 56 | 160 | +104 | holds the `Request` again |
| `execute_request_as` | 56 | 872 | +816 | the `Request` again (+104) and the moved-out `prepared` (712 B) still stored across the await |
| `execute_prepared_operation` (v3: forwarding wrapper) | – | 744 | +744 | `prepared` (712) and `context` stored again by an async fn that only forwards to `execute_finalized_operation` |
| operation body (v2.1 `execute_prepared_operation`, v3 `execute_finalized_operation`) | 1,152 | 1,808 | +656 | `prepared` 544 → 712 plus its destructured fields: head 416 → 512, journal 216 (new), controls 112 + ref 8, provider link 48, endpoint ID/method 48 + refs 16, meter 8, a second deadline/context for the finalization |
| work future (attempt loop, send, transport) | 3,304 | 3,872 | +568 | cancellation `select!` and poll wrapper (+40), `run_operation`/attempt state +312 (attempt environment, prepared body 80 → 152, journal/meter/controls references), `send_attempt` +216; the transport future (1,512) is identical |
| total | 4,624 | 7,616 | +2,992 | |

Why `Request` grew: `RequestOptions` 24 → 136 B, i.e. the per-request
`RequestControlState` (112 B: selected timeout, caps and cancellation).

| id | bytes | cause | class |
|---|---|---|---|
| X1 | +104 ×3 = 312 | `RequestControlState` in every `Request`; stored at three nested async levels | content (a) CONTROL-01/02 (selected timeout, caps, cancellation); the triple storage is rustc keeping each async fn's by-value argument in its state: avoidable layout |
| X2 | +712 | `execute_request_as` keeps the storage of the moved-out `prepared` across `execute_prepared_operation(..).await` | (b) avoidable layout; no functional reason (present since before d30e23d) |
| X3 | +744 | `execute_prepared_operation` became a forwarding `async fn` around `execute_finalized_operation` (F01, a20a331) and stores its arguments again | (b) avoidable layout (a plain `fn` returning the inner future, or calling `execute_finalized_operation` directly, would not store them) |
| X4 | +656 | operation state: journal (§13.1 EXEC-01), cancellation controls (CONTROL-01/02), provider link (EXEC-01, slice E), larger head and prepared operation, finalization references (EXEC-04, F01) | (a) required content; within it `prepared` is stored both whole and destructured, as in v2.1 (layout); the +96 B of `PreparedHead` were not broken down per field |
| X5 | +568 | work future: caller cancellation (CONTROL-01/02), attempt environment and phase state (§13.2), finalization inside the cancellation scope (EXEC-04) | (a) required content, not broken down further |
| X6 | +352 | buffered acquisition: `ResponseInput` 408 → 520 B (stream context 112 → 216 B, the §15 selected finite lease / STREAM-02 lifetime and idle state; response +8), stored three times (`acquire_buffered` argument, `collect_bounded` argument and local) = +336, +16 in the inner timeout future | content (a) §15; the triple storage is the same argument-storage layout as in v2.1 |

Time cost of the bytes: callgrind +3,465 instructions per execution (+4.3%),
of which +2,866 are `memcpy` (moving the larger coroutine states: +1,713 in
the boxed `into_future` state machine, +796 in `run_operation`, +576 in
`acquire_buffered`); the remaining concord deltas roughly cancel (for example
v2.1's `mime` Accept parse and `validate_accept` versus v3's allocation-free
validator, journal emission and stream-context setup). Atomics are 2.75 per
execution lower in v3. The warm median is +2.3% / +2.9% per block (bimodal
execution times on this host, see the r2 file).

Estimate (from the layout, not measured): removing X2 and X3 would shrink the
boxed future by about 1,456 B (to about 6,160 B), leaving execution about
+1,900 B/op above v2.1.

### Rate-policy planning: +330 B/op at six fewer allocations

Unchanged since d30e23d (4,213 B/op at every bisected commit). From the
backtraces: the rate-specific allocations are 24 / 1,205 B in v2.1 (push-time
origin normalization twice, ID, canonical parts, schema-free policy list) and
19 / 1,559 B in v3 (+354 B); v2.1 additionally parses the static `Accept` at
plan time (24 B, absent in v3, P5): 354 − 24 = +330.

| id | bytes (v3) | cause | class |
|---|---|---|---|
| R1′ | 3 allocations, 672 B | declaration phase (R1): dimensions `Vec` 224 B, windows `Vec` 96 B, declaration list 352 B, each holding ONE element after `Vec`'s first push reserved capacity 4 (4 × 56, 4 × 24, 4 × 88) | count (a) §12.1; 504 of the 672 B are unused capacity: (b) avoidable bytes (exact reservation would put the row about 170 B below v2.1) |
| R3′/R4′/R6′ | 16 allocations, 887 B | origin, key parts (a `Vec` of capacity 4 shrunk into a boxed slice: 224 B then a 56 B realloc), key bytes, schema descriptor and `Arc`, plan summary | (a) §12.5, §18.4 (as in the first round) |

v2.1's rate part also contains capacity-4 first-push vectors (the 256 B policy
list, 128 B parts), so the two versions pay the same growth policy; v3 has
more small vectors in the declaration phase.

### First use

| first operation (isolated fresh process) | v2.1 | v3 | description share (v3) | rest |
|---|---|---|---|---|
| plan (show) | 53 / 4,178 B, 30.6 µs | 527 / 71,305 B, 144.0 µs | 466 / 67,383 B | 61 / 3,922 B (warm plan 48 / 3,378 B plus one-time statics) |
| execution | 94 / 65,727 B, 528 µs | 162 / 81,556 B, 611 µs | 68 / 12,901 B | +3,344 B future sizes (X1-X6), −416 B smaller one-time items of v2.1 |

- **F1 (first plan, +466 allocations, +67 KB, about +95..115 µs).** The first
  plan of a client in a process builds that client's lazily allocated static
  contract description (`inspection::resolve`, all owners/endpoints/policies of
  the client; largest sites `compose_family` 26 allocations / 18,816 B,
  `build`, `collect_owners`, `contract::endpoint`); 122 allocations stay
  retained (peak live bytes during the plan 25,800 B, an upper bound for the
  retained part). §18.1 allows static metadata to be "lazily allocated once";
  the facts are §18.2 content. Class: not a steady-state hot-path regression
  (warm plans are at −1 allocation), but a real first-use cost of the current
  lazy form. It is avoidable in principle (emit the description as static
  data, or let planning read per-endpoint facts without building the whole
  client contract); not changed here.
- **F2 (first execution, +68 allocations, +12,901 B).** The same lazy
  description for `ExecApi` (a different client type), built on its first
  execution; the rest of the first-execution delta is the warm future-size
  delta.
- **F3 (later first plans).** Once a client's description exists, the first
  plan of another endpoint costs that endpoint's own lazy description: 12
  allocations / 300 B (route shape and display strings) for search, versus 4 /
  1,376 B of lazily generated endpoint metadata in v2.1; times are within
  +0.2..+0.9 µs of v2.1.
- Client build: +4 allocations, +304 B; time equal within noise.

### Planning time beyond instructions

Show, search and protected are +4..+7% in time at +0.6%, +0.1% and −0.6%
instructions and equal or fewer atomics except show (+4 Ge per plan; the
first round attributed the show row's extra atomics to the §7.3 snapshot join
of an unbound intent, A2). The cache simulator shows more L1i
misses per plan in every planning row (show 553 → 716, search 580 → 721,
protected 607 → 740, limited 733 → 935), consistent with a larger code
footprint on the planning path; simulated conditional/indirect mispredictions
move in both directions (search indirect 72 → 92, show 67 → 64). As in the
first round this is simulator evidence without hardware counters, and the
planning-time residual remains **partly unexplained**.

### Scale

Expanded code and description sizes are identical to d30e23d in both
dialects. At 1000 endpoints the whole-crate build is +33.0% (+28.0% in the
earlier file), local edit +35.9%, parent edit +38.6%, cold (fixture package
only) +33.7%; unstripped debug binary +28.96% (v3 +112,384 B since d30e23d).
Absolute times of both dialects are 10-20% higher than in the earlier block
(byte-identical v2.1 sources), and single 1000-endpoint runs vary by about
±5%, so the change from +28% to +33% is not attributed; larger monomorphized
execution futures per endpoint (X2-X5) are a plausible contributor, not a
measured one.

### Summary for the V3-O07 decision

- Counts: at or below v2.1 in every warm row. Bytes: +8 B (construction),
  −24 B (three plan rows), +330 B (rate plan), +3,344 B (execution).
- Newly required functionality: CONTROL-01/02 request controls, the §13
  journal and phase state, the EXEC-01 provider link, EXEC-04 finalization
  inside the cancellation scope, the §15 stream context, the §12.1/§12.5/§18.4
  rate declaration and schema, and the §18.1/§18.2 contract description.
- Avoidable, identified but not corrected (product code was out of scope):
  X2 (712 B) and X3 (744 B, introduced by the F01 repair) per execution; the
  triple by-value storage of `Request`/`ResponseInput` across async levels
  (part of X1/X6); unused `Vec` capacity in the rate declaration phase (504 B
  per rate-limited plan); and, as a design choice rather than a defect, the
  first-plan build of the whole client contract (F1).
- Unexplained: the planning-time residual above instructions (show, search,
  protected +4..+7%), the field-level growth of `PreparedHead` (+96 B), and the
  scale step from +28% to +33% at 1000 endpoints.

## r3 layout correction (review 3 §7)

Deterministic size probe only (`tools/perf/src/bin/future_sizes.rs`; no
timing): record `future_sizes_linux_2026-09-27_r3.txt`. The boxed operation
future of the managed execution falls from 7,616 B to **5,840 B** (v2.1:
4,624 B) and warm execution from 27,991 to **26,215 B/op** (v2.1: 24,647) at
63 allocations in all three trees. Changes, all in
`crates/concord-core/src/execute/mod.rs`, ordinary ownership only:

| id | change | measured |
|---|---|---|
| X2 | `execute_request_as` moves `prepared` into the operation future inside the `Ready` arm, so no moved-out `PreparedOperation` slot lives across the await | −712 B |
| X3 | `execute_prepared_operation` is a plain `fn` returning `execute_finalized_operation(.., identity finalizer)`; the F01 finalizer path is untouched | −744 B |
| X1 (two of three copies) | the `execute_request` forwarder and the `IntoFuture` `async move` block are gone; every terminal boxes `execute_request_as(request, output)` with a zero-sized output map (`ApiResponse::into_value` or `identity`) | −160 B, −160 B |

The attributed 1,456 B (X2 + X3) is recovered exactly; X1 recovers 320 B more
(this also removes two 56 B levels that v2.1 has). Laziness is unchanged:
every returned future is still an `async fn` coroutine that does nothing
before its first poll, so preparation (operation identity, deadline start)
still happens on first poll.

Remaining execution delta versus v2.1: +1,568 B/op = boxed operation future
+1,216 B (X1 last copy +104: `execute_request_as` must own the `Request`
until its first poll prepares it; X4 +656 and X5 +568 required content; −112
for the two v2.1 levels no longer present) + buffered acquisition +352 (X6,
not changed). A reverted experiment (using `PreparedOperation` fields in place
instead of destructuring them in `execute_finalized_operation`) grew the
future by 80 B, so the whole-plus-destructured storage inside X4 is not
removable by that change; it and the X6 triple `ResponseInput` storage are
the remaining layout opportunities.

## r3 remeasurement (review 3 §7; R3 instructions)

Three trees, built with the same toolchain, features, targets and workload in one
session on one otherwise idle 2-CPU host: v2.1 (`git archive baseline-v2.1` =
eefca26, plus byte-identical probe copies as in r2), the reviewed v3 (`git
archive 4c7b894`; `head_first_use.rs` present there, `future_sizes.rs` copied in
byte-identically) and the corrected v3 (**1477d3a**, clean; it includes the r3
layout correction). After the measurements one product change was made
(**2b82432**, below) and the warm, first-use, size and scale blocks were
repeated against all trees. Files: `head_cost_linux_2026-09-27_r3.txt`,
`first_use_linux_2026-09-27_r3.txt`,
`future_sizes_linux_2026-09-27_r3_remeasurement.txt` (the `_r3.txt` name is the
earlier layout record, kept unchanged), `expansion_scale_linux_2026-09-27_r3.txt`.
These are measurements, not an acceptance.

Benchmark-relevant differences: `git diff --stat 4c7b894 1477d3a -- crates
tools/perf/src tools/perf/benches` lists only
`crates/concord-core/src/execute/mod.rs` and the new
`tools/perf/src/bin/future_sizes.rs`; 2b82432 adds
`crates/concord-core/src/redaction/names.rs` and its test. 4c7b894's `crates/`
equal r2's 89b2cbc, and the rebuilt 4c7b894 and v2.1 probe executables are
byte-identical to r2's.

Method notes. The CPU model string differs from r2's host ("@ 2.80GHz" versus
"@ 2.10GHz"), so absolute times are not comparable with r2; only
same-session interleaved comparisons are used. Each tree has its own target
directory: with one shared directory cargo silently reused the worktree's
artifacts for the 4c7b894 export (identical package IDs and workspace-relative
paths), and that build block was discarded (disclosed in the files). Warm:
36 runs per tree (block A) and 32 (block B), tree order rotated every round.
First use: 20 fresh processes per tree and category plus 20 `sequence`
processes (blocks A and B) and a 60-sample A/B (block C). Scale: full sweeps
for v2.1, 1477d3a and 2b82432 and the 1000-endpoint row (default and
incremental) ×3 per tree and session. Instruction counts: callgrind over whole
probe runs; compile phases: `rustc -Ztime-passes`.

### Summary (1477d3a, like for like)

| row | allocations v2.1 → 4c7b894 → 1477d3a | bytes/op | time 1477d3a vs v2.1 (A / B) | time 1477d3a vs 4c7b894 (A / B) |
|---|---|---|---|---|
| direct construction | 5 → 5 → 5 | 168 → 176 → 176 | −9.7% / −9.3% | +0.4% / +0.7% |
| unbound construction | 2 → 2 → 2 | 80 → 80 → 80 | −4.8% / −4.3% | +2.0% / +1.5% |
| binding | 0 → 0 → 0 | 0 | −16.3% / −16.3% | 0.0% / 0.0% |
| plan: show | 49 → 48 → 48 | 3,402 → 3,378 → 3,378 | +6.2% / +7.2% | +2.3% / +1.1% |
| plan: search | 82 → 81 → 81 | 6,143 → 6,119 → 6,119 | +4.1% / +4.6% | +1.5% / +1.9% |
| plan: protected | 41 → 40 → 40 | 3,014 → 2,990 → 2,990 | +7.1% / +8.4% | +2.2% / +2.4% |
| plan: limited (rate) | 62 → 56 → 56 | 3,883 → 4,213 → 4,213 | +0.9% / +0.7% | +1.3% / +1.3% |
| managed execution | 63 → 63 → 63 | 24,647 → 27,991 → **26,215** | +8.1% / +2.9% | +0.1% / +3.7% |
| boxed operation future | | 4,624 → 7,616 → **5,840 B** | | |
| first plan (isolated, fresh process) | 53 → 527 → 527 | 4,178 → 71,305 → 71,305 | 4.1×–4.9× (126–144 µs vs 27–32 µs) | within ±10% |
| first execution (isolated) | 94 → 162 → 162 | 65,727 → 81,556 → 79,780 | 1.12×–1.20× | within ±10% |
| build, 1000 endpoints (default) | | | +34.1% (edit +38.6%, parent +36.3%, cold +38.4%) | −1.6%..+0.8% |

### What the r3 data establish

- **Layout correction.** Execution bytes fall by exactly 1,776 B/op at 63
  allocations; the remaining +1,568 B/op versus v2.1 is attributed in "r3
  layout correction" (+1,216 B operation future, +352 B buffered
  acquisition). The instruction count per execution falls 1.7% (callgrind),
  but no execution time change versus 4c7b894 is established (+0.1% and
  +3.7% in the two blocks; bimodal execution times on this host).
- **Planning residual is partly layout.** 1477d3a and 4c7b894 have identical
  planning source; callgrind counts 0.0–0.2% fewer instructions per plan for
  1477d3a, yet 1477d3a plans are +1.1% to +2.4% slower in both interleaved
  blocks. Moving code in the same crate (the execute/mod.rs change) thus
  shifts plan time by about 2% without any change in work. This is direct
  evidence that part of the +4–8% planning residual versus v2.1 (at equal
  instructions, r2: +0.6%, +0.1%, −0.6%) can be code placement, and it keeps
  the ±5% time resolution. The rest of the residual stays as attributed in r2
  (L1i and indirect-branch pressure from the dyn-dispatched patch-program
  evaluator, simulator evidence only); it remains **partly unexplained**.
- **First use is once per generated client type per process.** The first
  plan of a client type in a process builds that type's complete static
  contract description (`contract()` is a `static LazyLock`); later plans,
  other endpoints' first plans (sequence mode: within +0.1..+1.0 µs of v2.1)
  and further clients of the same type do not rebuild it. ExecApi, a
  different type, pays its own on first execution (+68 allocations,
  12,901 B).

### Investigation: first-plan description build (1477d3a)

callgrind, one fresh `plan-show` process: the contract initialization is
363,095 instructions (the plan itself 35,572), of which
`inspection::resolve::build` 334,091:

| site | instructions | note |
|---|---:|---|
| `compose_family` (24 calls: `owner_taints` 12, `resolve_endpoint` 8, `owner_view` 4) | 143,483 | every owner's query/header composition is computed up to three times |
| – of which `redaction::is_sensitive_name` (32 calls) | 83,854 | ~2,600 instructions per call: a new normalized `String` plus 15 substring searches |
| `resolve_endpoint` (incl. its compositions) | 92,000 | |
| `owner_taints` (incl. its compositions) | 77,801 | |
| `collect_dependencies` | 46,121 | 21 `format!` calls (23,821): dependency text content |

The same classifier is on the warm path, shared with v2.1 in identical
form: 4 calls per warm plan on average (336,016 calls in 84,004 plans),
about 19% of all instructions of a head_cost run in both v2.1 and v3.

**Change made (2b82432, `redaction/names.rs`).** `is_sensitive_name`
compares each window of the name with the ASCII categories, normalizing bytes
in place (`_` → `-`, ASCII lowercase) instead of collecting a normalized
`String`. Because every category is ASCII, a match can never involve a
non-ASCII byte, so the result equals the old substring search of the
normalized copy; a new differential test
(`redaction_matrix_in_place_normalization_equals_the_normalized_copy`)
checks the two on names with underscores, mixed case, truncations, control and
non-ASCII characters. No work is skipped and the classification is unchanged.
Tests: `cargo test -p concord-core --lib --all-features` (897 passed),
`cargo test -p concord --all-features --test generated --test runtime` (168 +
73 passed), perf `owner_snapshots` (3 passed); clippy `-D warnings` and fmt
clean.

Measured effect (block B/C, callgrind):

| | 1477d3a | 2b82432 |
|---|---|---|
| first plan allocations (show / search / protected / limited) | 527 / 556 / 509 / 525 | 492 / 517 / 474 / 490 |
| first plan: contract initialization + plan, instructions | ~399,000 | ~346,000 (−13%) |
| first plan time | | not established: +2.6..+4.3% (B, 20 samples), −0.1..+0.9% (C, 60 samples) |
| warm plan allocations (show / search / protected / limited) | 48 / 81 / 40 / 56 | 45 / 74 / 37 / 53 |
| warm plan instructions (show / search) | 34,169 / 55,993 | 29,684 / 48,982 (−13.1% / −12.5%) |
| warm plan time vs 1477d3a (show / search / protected / limited) | | −12.2% / −11.6% / −13.6% / −9.1% |
| warm plan time vs v2.1 | +7.2% / +4.6% / +8.4% / +0.7% | −5.9% / −7.5% / −6.3% / −8.5% |
| execution, construction, binding, first execution | | unchanged (counts, bytes; time within noise) |

Two cautions. (1) The warm gain comes from making a helper cheaper that v2.1
has in identical form; it offsets, and does not remove, the v3-specific
planning residual, whose like-for-like size is the 1477d3a comparison.
(2) The first-use time did not follow the 13% instruction reduction: on this
host a fresh-process first plan (cold caches and predictors on code never run
before) is evidently not instruction-bound at this size; why was not
established (no hardware counters; minor page faults differ by only +1
(v2.1) / +4 (v3) between a planning and a non-planning category).

What remains avoidable in principle but was not changed (not local): each
owner's family composition is recomputed for `owner_taints`,
`resolve_endpoint` and `owner_view` (94,556 instructions of compositions
remain at 2b82432; caching them per owner would restructure the resolver),
and, as a design choice, building the whole client contract on the first
plan instead of emitting it as static data or reading per-endpoint facts
(r2 F1). Neither is a steady-state cost.

### Investigation: build time at scale

`-Ztime-passes` on the 1000-endpoint fixture (single runs): total 19.9 s
(v2.1) → 26.9 s (v3). Type checking +3.9 s, borrow checking +4.6 s and
coherence +1.1 s (+8.5 s together) against code generation and LLVM −3.8 s;
macro expansion equal (2.11 / 2.14 s). The build regression is therefore
front-end checking of the larger generated item set (+50% declaration-shaped
and +65% function-shaped lines at +4% expanded lines), not code generation:
the r2 conjecture that larger monomorphized execution futures per endpoint
contribute is not supported (the back end is cheaper in v3). Reducing it
means generating fewer or simpler items (typed owner projections and value
readers, per-endpoint request/intent/view types, the value-control matrix,
the static description) — a generator redesign, not done here. The three v3
trees agree within the ±4% single-run spread at 1000 endpoints (1477d3a
versus 4c7b894 −1.6%..+0.8%, 2b82432 versus 4c7b894 −1.6%..−5.6%);
incremental parent edits stay faster than v2.1 (−12% to −15%).

### Status after r3

- Counts at or below v2.1 in every warm row; bytes +8 B (construction), −24 B
  (three plan rows; −56..−88 B at 2b82432), +330 B (rate plan; +298 B at
  2b82432), **+1,568 B** (execution, was +3,344 B).
- Known avoidable layout in execution (X2, X3, two X1 copies) is removed; the
  remaining execution bytes are required content (X4, X5, one X1 copy) and
  the X6 triple `ResponseInput` storage (not changed).
- First use: once per client type per process, +0.1 ms class for this
  fixture; −35 allocations and −13% instructions at 2b82432, time unchanged
  within resolution; the whole-contract lazy build remains a design choice.
- Planning time: like for like +1% to +8% (1477d3a), of which about 2% is
  shown to move with code placement alone; at 2b82432 −6% to −9% versus v2.1
  through a shared-helper improvement.
- Build: +29% to +39% at 1000 endpoints, all in front-end checking of the
  generated code.
