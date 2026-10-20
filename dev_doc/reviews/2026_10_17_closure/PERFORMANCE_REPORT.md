# Concord V3 performance qualification

## Decision scope and reproducibility

This report evaluates the selected V3 architecture and the corrected source,
not equality with a different historical feature set. The measurements use
579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31. Its library, macro, example and
performance inputs are the corrected V3 product; later changes to acceptance
oracles, support CLI and reporting receive separate native qualification.
The final IntoValue sentence edit is proved to change one documentation comment
only and is regenerated with native Rustdoc; it does not change measured code.
No old-version compatibility was added to the product.

The three comparison sources are:

| Label | Commit | Purpose |
|---|---|---|
| Historical | 3765907aa2e424997bfd8f2fcdcc95275e6142f5 | Isolated historical reference for the cost comparison |
| V3 before | ae6006184f3dc903fdfe66c572098ac810e2c6c5 | Owner-pushed baseline before this correction pass |
| V3 final | 579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31 | Corrected product measured here |

[The paired campaign](https://github.com/gqsnt/concord_api/actions/runs/38981477842)
and [the attribution campaign](https://github.com/gqsnt/concord_api/actions/runs/38983860434)
retain their actual commands, exits, toolchains, environment, source snapshots,
lockfiles, harnesses, fixture inputs, expansion outputs, binary hashes and raw
native logs. Each run has a different evidence role.

The runtime and scale modes used separate Ubuntu 24.04 runners. Each mode
compared its three sources on the same runner, with Rust 1.97.1 and 188
external package identities aligned by name/version/source/checksum. Rustls
0.23.46 and WebPKI 0.103.15 were aligned. Historical path manifests were
preserved except for the declared comparison setup. The runtime probes use
the same final harness; the historical declaration is wire-equivalent for
the compared bases/routes/query/auth/rate cases, including X-Client-Version=3.
The historical grammar stays inside its detached measurement worktree.

Runtime has five independent process repetitions per case with counterbalanced
order. The campaign records 180 measurement processes and 191 native command
receipts, all successful. Scale records three full 1/10/100/500/1,000-endpoint
runs plus two further before/final pairs at 1,000 endpoints: seven measurement
processes, 19 measured points and 18 native receipts, all successful. Only the
two intentional Rustls-alignment commands in each preparation changed inputs.

The actual measurement commands run with network-offline Cargo after
preparation. The original recorder's selected-environment subset omitted
CARGO_NET_OFFLINE; the initial environment was captured before preparation.
This limits what the original receipt alone proves about that variable.
The source imposes it before measurements, and the later attribution job
captures its true value after preparation. No original metadata is rewritten
to claim an observation that was not recorded.

## Runtime, allocation and future size

Runtime timing is release-mode loopback HTTP with keep-alive. Allocation
instrumentation counts the client thread, not all server/runtime threads or
process RSS. Warm ns/op values are medians of the probe's batch medians across
five process runs. They are not remote network, TLS or provider latency.

| Warm operation | Historical allocations / bytes | V3 before allocations / bytes | V3 final allocations / bytes |
|---|---:|---:|---:|
| Direct request construction | 5 / 168 | 5 / 176 | 5 / 176 |
| Unbound intent construction | 2 / 80 | 2 / 80 | 2 / 80 |
| Binding | 0 / 0 | 0 / 0 | 0 / 0 |
| Path head plan | 49 / 3,402 | 46 / 3,382 | 46 / 3,382 |
| Query head plan | 82 / 6,143 | 75 / 6,091 | 75 / 6,091 |
| Protected head plan | 41 / 3,014 | 38 / 2,994 | 38 / 2,994 |
| Rate-limited head plan | 62 / 3,883 | 54 / 4,217 | 54 / 4,217 |
| Managed buffered execution | 63 / 24,647 | 63 / 28,402 | 63 / 27,874 |

The outer execution future is 4,624 bytes historically and 8,016 bytes in
both V3 versions. The warm managed-execution timing medians are 50,046,
51,954 and 51,862 ns/op respectively. The final process results range from
50,651 to 58,866 ns/op. This does not establish a general latency improvement.
The definite execution saving is 528 allocated bytes per operation, with
the allocation count and outer future size unchanged.

The first isolated plan initializes the V3 static contract description:

| First plan | Before allocations / bytes | Final allocations / bytes | Before median ns | Final median ns |
|---|---:|---:|---:|---:|
| Path | 489 / 70,917 | 434 / 61,451 | 112,557 | 105,977 |
| Query | 514 / 73,546 | 459 / 64,080 | 115,791 | 110,083 |
| Protected | 471 / 70,234 | 416 / 60,768 | 115,562 | 109,492 |
| Rate-limited | 487 / 71,443 | 432 / 61,977 | 113,268 | 107,539 |

Each of these first-plan cases saves 55 allocations and 9,466 allocated
bytes. Peak live bytes increase slightly, from 25,800 to 26,088, because the
optimized builder retains intermediate composition results before moving
them into final views. Reduced allocation traffic is not a claim that every
memory metric falls.

The corresponding historical first plans take approximately 25–29 microseconds
with much smaller descriptions. The sequence probe distinguishes first use
from later endpoint access: after the first path plan initializes the V3
description, query/protected/rate plans use 87/44/60 allocations and
6,391/3,079/4,288 bytes. Those counts are identical before and after this
correction. The initialization cost is not charged afresh to every endpoint.

Client construction has 5,143 allocations and 6,727,448 allocated bytes in
both V3 versions; first execution has 161 allocations, with 81,957 before
versus 81,525 median bytes after. Complete samples, including net allocation
counts and peaks, are in PERFORMANCE_RUNTIME.csv. Negative net counts for
construction/binding reflect destruction of inputs allocated before the
measured window; they are not leak measurements.

## Controlled runtime attribution

The attribution source remains the measured final product. Separate temporary
worktrees restore exactly one old mechanism, without entering the product
branch. One restoration preserves V3 diagnostic wording while restoring the
old repeated composition algorithm. A third diagnostic variant instruments
the complete description build with eight Instant intervals; it does not
omit any selected feature.

Five native before/after mechanism pairs confirm the following:

| Mechanism restored in isolation | Restored | Corrected | Interpretation |
|---|---:|---:|---|
| Buffered acquisition future allocation | 2,728 bytes | 2,200 bytes | Removing the redundant async wrapper saves exactly 528 bytes |
| Managed execution allocated bytes/op | 28,402 | 27,874 | Same 528-byte saving in all five runs; 63 allocations in both |
| First query plan allocations | 514 | 459 | Avoid repeated owner query/header composition |
| First query plan allocated bytes | 73,546 | 64,080 | 9,466-byte saving, stable in all five runs |
| First query plan retained allocation count | 122 | 122 | No claim of reduced retained object count |
| First query plan peak live bytes | 25,800 | 26,088 | Explicit 288-byte peak tradeoff |

The isolated first-query timing medians are 117,761 ns with repeated
composition and 114,355 ns after the correction. These diagnostics support
causality for the exact allocation differences. They are not substituted for
the main uninstrumented performance campaign.

The wrapper contains a 520-byte ResponseInput plus the 2,200-byte inner
future and discriminant/padding; its native size is 2,728. The corrected
function directly implements that inner acquisition state. The outer operation
still awaits a boxed response driver, so its own 8,016-byte layout does not
shrink. Historical ResponseInput is 408 bytes versus 520 in V3; the native
fields show a 112-byte old observation value replaced by a 216-byte V3 stream
context, plus an 8-byte response change. PreparedRequestBody includes 24 bytes
of V3 body-description facts. These are explicit current semantics, not an
older-version adapter.

The instrumented description intervals have these per-stage medians:

| Complete-build interval | Median ns |
|---|---:|
| Tree, values and references | 22,513 |
| Validation and attachments | 6,482 |
| Head composition | 8,076 |
| Effective endpoint views | 13,004 |
| Cycles, coverage and liveness | 6,974 |
| Redaction and owner views | 2,674 |
| Dependencies and diagnostics | 11,491 |
| Result assembly | 2,975 |

The five complete measured interval totals are 75,723, 72,175, 98,195,
98,324 and 74,290 ns. This locates the first-use work in the full static
description construction, rather than in request network execution. Individual
stage medians must not be summed into a synthetic observed total.
Instrumentation adds clock reads and emits after construction; its total
operation timing and allocation values are excluded from comparisons with
the uninstrumented campaign.

## Expansion and compilation scale

Scale uses the generated required-value, body-free Binary/rate-limit fixture,
normal debug information, no stripping and CARGO_INCREMENTAL=0. Source and
expanded text are collected outside the timings. "Cold" means cleaning the
fixture package while dependency artifacts remain; it is not a fresh machine.
Description bytes are serialized JSON bytes, not heap occupancy.

The complete first-campaign final points are:

| Endpoints | Expand ms | Build ms | Local edit ms | Parent edit ms | Fixture cold ms | Warm ms |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 368 | 396 | 295 | 299 | 280 | 57 |
| 10 | 95 | 394 | 407 | 397 | 400 | 60 |
| 100 | 263 | 1,465 | 1,476 | 1,521 | 1,425 | 54 |
| 500 | 1,018 | 6,533 | 6,503 | 6,579 | 6,516 | 52 |
| 1,000 | 2,056 | 13,130 | 13,352 | 13,322 | 13,155 | 59 |

The one-endpoint expansion value is a single outlying observation: before
is 87 ms, historical 81 ms, final 368 ms. It is retained in the table, not
discarded or assigned an invented CPU/filesystem cause. A persistent regression
at that point is not established by one sample. No workload-independent
performance promise is made from these small-point timings.

At 1,000 endpoints, the additional paired runs give:

| Measurement | Historical, one run ms | Before samples ms | Final samples ms | Before / final median ms |
|---|---:|---|---|---:|
| Expansion | 2,351 | 2,275; 2,186; 2,132 | 2,056; 2,209; 1,936 | 2,186 / 2,056 |
| Build | 11,034 | 14,714; 14,394; 13,128 | 13,130; 13,316; 12,118 | 14,394 / 13,130 |
| Local edit | 10,761 | 14,410; 14,984; 13,988 | 13,352; 13,461; 12,262 | 14,410 / 13,352 |
| Parent edit | 10,658 | 14,523; 13,797; 12,820 | 13,322; 13,590; 12,150 | 13,797 / 13,322 |
| Fixture cold | 10,798 | 14,424; 13,279; 13,147 | 13,155; 13,098; 12,556 | 13,279 / 13,098 |
| Warm | 55 | 57; 54; 51 | 59; 54; 50 | 54 / 54 |

Build median improves by 8.78%; local-edit median by 7.34%. Some parent/cold
ranges overlap. The results do not establish a universal build-time gain.
V3 retains a material front-end compilation cost relative to the historical
fixture, which the native phase profiles below locate.

At 1,000 endpoints the expansion counters are:

| Counter | Historical | V3 before | V3 final |
|---|---:|---:|---:|
| Lines | 332,393 | 355,841 | 349,852 |
| Nonindentation characters | 15,754,689 | 12,784,600 | 13,036,120 |
| Token-like fragments | 922,275 | 1,143,250 | 1,107,250 |
| Declaration-shaped entries | 44,057 | 67,064 | 65,064 |
| Function-shaped entries (metric name: methods) | 20,019 | 34,037 | 32,037 |
| Unstripped debug binary bytes | 43,812,120 | 57,196,336 | 57,255,784 |
| Serialized description bytes | 1,285,364 | 5,824,068 | 5,824,068 |

The correction removes two unused generic helpers per required-only endpoint:
2,000 helpers at this scale. Current V3 description JSON is byte-identical in
size before/after. Identifier hygiene uses longer private names, so characters
increase despite fewer declarations. The final debug binary is 59,448 bytes
larger, not smaller. Richer V3 description JSON is not compared as equal-feature
heap usage with the historical description.

The function-shaped counter is a text heuristic, not a count of public APIs or
every Rust function. For this specific fixture the V3 request wrapper contributes
seven more counted functions than the historical shape: current control and
description methods, RequestControls forwarding and Clone, with the old debug
method removed. The endpoint/storage group contributes five more counted
functions through the separate Local/Spec/View/intent shapes, owner snapshots,
head emission and bound/unbound planning. The heuristic omits an older
pub(in ...) constructor. These references explain the counter's meaning;
they do not assign a measured time to each feature. The fixture has no body,
credentials or pagination, so those features are not blamed for its delta.

## Native compiler attribution

Nine profiles compile the actual archived 1,000-endpoint base/local/parent
fixtures for each source. Every selected source/lockfile hash is verified
against the campaign archive. Only declared path dependency roots are remapped.
Dependencies remain aligned, debug is normal/unstripped, incremental is off,
and RUSTC_BOOTSTRAP enables diagnostic -Ztime-passes. Input hashes before and
after every profile match. All nine compiler commands and nine package-clean
commands exit zero.

Selected base-fixture phase wall times are:

| Phase | Historical s | V3 before s | V3 final s |
|---|---:|---:|---:|
| Macro expansion | 1.245 | 1.291 | 1.259 |
| Type checking | 3.694 | 6.045 | 5.727 |
| MIR borrow checking | 2.690 | 5.552 | 4.785 |
| LLVM passes | 2.701 | 1.390 | 1.251 |
| Compiler total | 12.399 | 16.689 | 15.367 |

Across base/local/parent, before MIR borrow checking is
5.552/5.129/5.324 seconds; final is 4.785/4.635/4.569. Final type checking
is 5.727/5.977/5.733 versus historical 3.694/3.818/3.799.
The remaining V3 difference is concentrated in the generated front-end
type/borrow workload; macro expansion is close and LLVM is lower in this
fixture. The removed helper functions reduce that workload, consistently
with the main paired timings. The source pass changes other correctness
details too, so the total time reduction is not assigned solely to one helper.

Phase wall times can nest or overlap. They must not be added as independent
CPU costs; coherence is included within the type-checking work. Each edit
has one diagnostic profile, while the repeated uninstrumented campaign owns
the timing trend. Native lines, including the compiler's own RSS observations,
are preserved in PERFORMANCE_COMPILER.csv and the original artifact.

The first parser retained the RSS prefix in phase names because this Rustc
format separates the final phase with a tab. The correction extracts the
actual phase name from the unchanged native line. Native seconds and lines
remain identical, the old artifact remains available, and an actual-format
regression test covers this normalization.

## Why the outer V3 future remains larger

The diagnostic capture reads the original native type-layout files from the
attribution archive, verifies its SHA-256, and exposes selected fields without
recompiling or changing the measured source. The [capture job](https://github.com/gqsnt/concord_api/actions/runs/38985826352)
also retains the selected JSON and original member hashes.

| Retained state in the measured execution path | Historical bytes | V3 final bytes |
|---|---:|---:|
| Outer request future | 4,624 | 8,016 |
| Prepared-operation/finalized-operation future | 4,456 | 7,856 |
| Captured prepared operation in that future | 544 | 736 |
| Retained request head | 416 | 536 |
| Inner operation block | 3,304 | 5,960 |
| V3 operation journal | Different old counters/observation layout | 216 |
| V3 operation controls | Different old representation | 112 |
| V3 provider link | Different old representation | 48 |

These rows are nested layouts, not independent allocations or quantities to
sum. Historically the outer 4,624-byte future retains a 48-byte request and
awaits 4,568/4,512-byte adapter futures before the 4,456-byte prepared
operation. Its largest operation block is 3,304 bytes.

V3's 8,016-byte outer future retains a 152-byte request and awaits the
7,856-byte finalized operation, with state flags/alignment. The latter's
largest suspension state retains the 736-byte preparation, 536-byte head,
216-byte operation journal, controls and provider correlation, and a
5,984-byte set of futures with a 16-byte polling future. The nested operation
block is 5,960 bytes and awaits the 5,824-byte run_operation state. This places
the remaining cost in the operation state machine and its current cancellation,
completion and correlation context. It does not attribute every byte to an
individual semantic feature or prove that the compiler layout is minimal.

The separate 216-byte StreamContext consists of correlation (96), cancellation
(24), limits (56), operation cap (16), observer (16) and handoff (8).
PreparedRequestBody is 40 bytes versus the historical 16 because it retains
the current 24-byte body description. These native fields help distinguish
current behavior from an accidental extra async wrapper.

The outer increase is 3,392 bytes over the historical fixture and zero bytes
between the two V3 candidates. Restoring the removed response wrapper changes
neither outer size nor the 63-allocation count; it changes the response-driver
allocation by exactly 528 bytes. Boxing more of the outer operation would
trade inline future storage for an additional allocation or a different
lifetime boundary. No such architecture change is justified by these measurements
alone. The diagnostic layout is specific to the recorded target, compiler and
fixture; it is not a stable ABI promise.

## Evidence files and practical limits

PERFORMANCE_RUNTIME.csv contains all 133 metric rows with original samples.
PERFORMANCE_SCALE.csv contains all 19 points. PERFORMANCE_COMPILER.csv contains
all 333 native phase rows with normalized phase names. The final artifact
retains full raw logs and inputs, not merely these tables.

The measured product implements finite operation/stream/traversal bounds,
current body facts, explicit cancellation, observation and completion
semantics, and the rich static description. Those selected capabilities are
kept. No feature is removed or compatibility adapter introduced to improve a
benchmark. The results support the bounded corrections and an explained
V3 cost profile; they do not promise zero overhead, constant compilation cost,
remote latency improvement or universal memory reduction.

## Review disposition

The avoidable wrapper, repeated composition and unused-helper costs have
been corrected and checked with discriminating native evidence. The paired
campaign, isolated mechanism restorations, native compiler phases and actual
type fields explain the remaining differences sufficiently for machine
qualification of the selected V3 architecture.

The V3 first-use description, larger inline future, larger debug binary and
front-end compile cost remain explicit tradeoffs. They are not hidden,
converted into a parity claim, or treated as support for V2. Future optimization
can use the retained harness and exact samples; it should preserve the current
semantic contracts and demonstrate an improvement in the metric being targeted.
