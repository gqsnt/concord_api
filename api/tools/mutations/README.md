# Mutation gate

From `api/`, under the build memory limiter (the sandbox uses `tools/limited.py`):

```sh
just mutants --list
just mutants --jobs 1
just mutants M92 --packages all --jobs 1
just mutants M46 --with-sockets --jobs 1
```

The default packages are `concord-core`, `concord-extras`, `concord`, `tour`, and
`conformance`: the engine, facade, handwritten model and its contracts. `--packages
all` additionally includes the macro, testkit and generated model/contracts.
Every build uses `--offline --locked --all-features`. Only one worker is allowed;
Cargo uses four jobs. Library and integration test binaries run serially with
one libtest thread. Rustdoc tests are outside this runner.

By default, the explicit `SOCKET_TESTS` list excludes tests that bind listeners,
and the entire `concord-testkit::server` integration target is omitted. Nothing
is silently excluded based on its result. M46 is marked `requires_sockets` and is
left out of the default mutation queue. Selecting only M46 without
`--with-sockets` is an error. On the architect's socket-capable machine, the M46
command above includes redirect tests; `--with-sockets` without an ID runs the
entire catalogue, including M46. Explicit user exclusions remain in force.

## Trust after interruption

A run takes an exclusive invocation lock before copying sources. Every worker's
whole scratch directory, including both Cargo target directories, is removed
before baseline compilation. Copying pristine files alone is insufficient:
copy2 preserves old mtimes and Cargo can otherwise reuse an artifact compiled
from a killed run's mutation. The cost is **one cold baseline compilation per
invocation**. Artifacts are reused only between mutations within that invocation.
Sources are restored in `finally` after each mutation. Working sources are never
written. Children inherit the invocation lock, so killing the Python parent
cannot admit another run while its Cargo/test children are still alive.

The snapshot includes sibling `docs/` at the same relative path the macro tests
read. Direct test execution receives Cargo's library-search paths, including
Rust's target library directory (`libstd.so`) and the worker's `debug/deps`.
Macro test binaries that run nested Cargo/trybuild jobs receive the compilation
watchdog rather than the ordinary ten-second test watchdog.

An unmodified baseline must compile and pass before any mutation runs. Invalid
catalogues, baseline failures and infrastructure failures abort with exit 2.
`--exclude-test-target PACKAGE::TARGET` and `--skip-test NAME` are explicit,
repeatable opt-outs, recorded in the run report. `--timeout` bounds an ordinary
test binary (default 10 seconds); `--build-timeout` bounds compilation and macro
test binaries (default 180 seconds). Watchdogs kill the whole subprocess group.

A mutant is caught when it fails to compile, fails a test, or reaches a watchdog.
Survivors and stale anchors produce exit 1. The final line reports caught,
survived, stale and socket-only counts. Logs and selections are retained in
`target/mutants/last-run.{txt,json}` and `target/mutants/logs/`. All snapshots,
compiler output and temporary files stay beneath `api/target/mutants/`.

## Data format

One UTF-8 JSON object per `Mxx.json` or `Rxx.json`, with these required string fields:

| Field | Meaning |
| --- | --- |
| `id` | Stable identifier, matching the filename. |
| `area` | Area used to group coverage gaps. |
| `contract` | Contract number or specification page/section being broken. |
| `description` | One line describing the caller-visible fault. |
| `file` | Source path relative to `api/`; no absolute paths or `..`. |
| `find` | Exact original UTF-8 text, including whitespace. |
| `replace` | Exact replacement text; may be empty. |

The optional boolean `requires_sockets` marks a fault that needs real listener tests.

The find text must occur exactly once in the snapshot. Zero or multiple matches,
or a missing source file, produce `STALE` and fail the run. Context belongs in the
find text when needed to distinguish repeated code. A mutation is applied alone;
never stack mutations or update expected values in tests.

## Provenance

M01–M50 preserve the reader's fifty mutation ideas; none was dropped. Anchors
follow current engine code, including the shared typed/raw retry loop and its
retained final outcome. M16/M17 target the deferred-join body reader; M18/M49
retain their mismatch-evidence faults. M52 still changes percent escapes to
lowercase after the encoder stopped allocating format strings per byte.

M51–M100 add independent faults in construction, value writing, paging, readers,
Base, multipart and Entries. M92 now violates A50 by accepting carriage returns
in field/file names instead of rejecting finish(). M46 remains socket-only.

R03, R08, R11, R16, R17, R23 and R24 preserve the end-of-M2 reader's surviving
mutations verbatim (the original before/after strings are find/replace here).
R11 and R17 now have engine unit tests for A56/A57. R03 is a growth-strategy
coverage probe. R08, R16, R23 and R24 await the conformance writer's tests.

## Maintainer regressions (M101–M129)

These cover A59, A61–A63, error sources and API stability, stale exemptions,
contract names, builder must-use warnings, body-template depth and reader docs.
Optional `checks` select `surface`, `docs-core`, `docs-extras` or `allocations`.
Their union must pass on the pristine baseline; each mutant then runs its own
extra checks after the ordinary suite. Cargo commands remain sequential, with
four jobs. Allocation checks run `macro_get --test` (no timed Criterion samples),
requiring at most 23 added allocations for both GET rows plus
checking that a scalar template has no collection allocation. The cold baseline
also compiles that bench package once. All check logs stay with the mutant.

The E13 comparison endpoint is `tour-gen::Shapes::guarded`: one path input and two
query inputs (path/query keys are Secrets). No Shapes or Tasks endpoint in this
revision has the requested two path inputs and three query inputs. The legacy
handwritten GET is named `legacy/get/*`; `macro_get` now supplies the exact A63 budget
row named `get/build` and the secondary `guarded/build` row. Run `just bench --bench macro_get` for its time/count table.

## Follow-up reading (M130–M137)

M115 now anchors the exact release candidate version. M130 covers the borrowed
query iterator independently of M58's owned header iterator. M131–M137 cover
A73, A72, Bounded ownership and the facade Arg page/hidden blanket impls.
The `docs-pages` check renders fresh facade HTML and verifies the Arg page
and all facade type pages without blanket implementations. Its Cargo output shares the worker target.
R03 is now caught by the large-body amortized growth test.

The A63 record is the local `api!` Riot client in `concord/benches/macro_get.rs`:
two path inputs, three query inputs, a route host and a secret header. The
`Shapes::guarded` comparison remains the separate `guarded/build` row.

## Misuse reading (M138–M140)

These restore the excessive-URL transport failure (A74), the retryable
classification of malformed heads and failed uploads (A75), and the body-label
error that blames serialization (A76). The A75 default fixtures produce real
Hyper failures over memory I/O and retain them in real reqwest request errors
through an injected resolver; Script verifies the standard retry decision.
The wire-level complement is
`c6_2_a75_real_reqwest_malformed_heads_and_request_body_failures`, enabled with
`--with-sockets` and explicitly excluded otherwise.
