# Performance record — native header names and representation statements

Single machine: Intel i5-13600K (20 threads), Linux 7.0.0-42, CPU governor `powersave` (not
changed: a system setting), rustc 1.97.1, no `sccache`. The desktop was in use; timed phases
started only after three consecutive one-minute checks found no game, no cargo/rustc/hyperfine,
no Codex run of this campaign, at least 30 GB free disk and a load average below 3
(`/home/canarit/projects/concord_v4_finish_work/tools/perf_timed.sh`). A benchmark is not a correctness test: `cargo test -p
concord-bench` checks that each Concord request equals its hand-written counterpart (method, URL,
header multiset, body), including the new `keys` group.

| Identity | Meaning |
|---|---|
| `625ac49` | baseline: the native-representation candidate this campaign started from (`concord-v4-native-repr`) |
| `3b5c5ed` | candidate measured (native header names, sealed values, representation statements, the `keys` benchmark group, the `--keys native` generator option, the fresh-use fixes) |

Raw data (outside the repository, not copied into it):
`/home/canarit/projects/concord_v4_finish_work/evidence/perf/` — allocs_625ac49.{json,md},
allocs_389392f.{json,md}, allocs_diff.md, and timed-20261102T175423Z/ (Criterion estimates
and samples of every run under `criterion/`, summaries, hyperfine JSON under `compile/`,
environment.txt, log.txt).

## How to reproduce

```text
cargo run --release -p concord-bench --bin alloc-count -- --json allocs.json
cargo bench -p concord-bench --bench runtime -- --save-baseline <name>          # ~25 min
python3 -I benches/criterion/summarize.py --current <run> --baseline <name>
python3 -I benches/scale/gen.py suite OUT --concord <snapshot>/crates/concord --syntax formats|statements [--keys native]
RUNS=3 benches/scale/ab.sh RESULTS OUT_A OUT_B OUT_C && python3 -I benches/scale/ab_summary.py RESULTS
```

## 1. Allocations per `prepare` (exact, counting allocator)

Allocation counts do not depend on load, so they were taken first (`625ac49` and `389392f`; the
runtime code measured by them is unchanged up to `3b5c5ed` apart from the conflict error path).

- **All 84 benchmark IDs shared with the baseline: identical** allocation calls and bytes, inputs
  and stages (allocs_diff.md: 0 differences). The representation statements are a macro-time
  change; the sealed `ToHeaderValue`, the native `HeaderValue::from_str` path for text and
  `http`-based literal validation cost no allocation.
- **Native names cost nothing over literal names**: the `HeaderPlan` that composes a chain with a
  native constant is a stack array of flags.

| Scenario (`keys/…`, `prepare`) | literal names | native names | hand-written |
|---|---|---|---|
| `six_inherited` (6 headers, 1 sensitive, reused scope) | 13 / 911 B | 13 / 911 B | 4 / 728 B |
| `one_replaced` (one header replaced through another spelling) | 13 / 911 B | 13 / 911 B | 5 / 729 B |
| `riot_match_ids` (routed host, native sensitive key, one optional query; input 3 / 132 B) | — | 29 / 2,145 B | 8 / 1,228 B |

The gap to hand-written code (URL, the `Parts` vectors, request assembly) is the fixed overhead
recorded by the previous campaign (`../native-repr/PERF.md` §2), unchanged here; the Riot request
also formats its routed host (not attributed further).

## 2. Runtime (Criterion)

Three full runs on the idle machine, in sequence: `625ac49` (17:56–18:21Z), then `3b5c5ed` twice
(18:23–18:51Z, 18:53–19:21Z); default Criterion settings, one shared `CRITERION_HOME`. Summaries:
summary-base.md, summary-cand{1,2}-vs-base.md, summary-cand2-vs-cand1.md.

**Noise floor.** The hand-written (`manual`) benchmarks run identical code in every run, so their
changes are noise: median +1.1 % / −0.3 % against the baseline, 14 and 17 of 86 beyond ±5 %, and
extremes from −49 % to +92 % on the shortest IDs. Concord's 92 shared IDs: median +0.6 % / −0.1 %,
9 and 11 beyond ±5 %.

**Shared IDs: no change attributable to the candidate.** Seven Concord IDs moved by more than 5 %
in the same direction in both candidate runs. Three of them moved with their hand-written
counterpart: `body_encode/json/{10KB,1MB}/…/prepare` −5 to −9 % on both sides, and
`body_encode/text/1MB/…/prepare` +27 % against +20 %/+5 %. Three others are small in absolute
terms or noisy:
- `body_encode/text/100B/concord/input`: 14.3 → 11.5 ns, building the request handle;
- `body_encode/text/1MB/concord/input`: −8 %/−13 %, against +9 %/−7 % for its hand-written
  counterpart;
- `body_encode/bytes/1MB/concord/prepare`: +5 %, 11 ns.

The seventh, `headers/4/concord_text/prepare`,
is 6 % faster in both runs (260.6 → 245.5/244.1 ns). F07 removed Concord's own visible-ASCII
check in front of `HeaderValue::from_str`, which fits a small gain, but it was not isolated.
Round trips over loopback (`roundtrip/*`) are within ±5 % of the baseline on both sides.

**Native names: the run-time header plan costs about 140–180 ns per `prepare`** on these chains,
with no allocation (§1):

| `keys/…/prepare` (run 1 / run 2) | literal names | native names | hand-written |
|---|---|---|---|
| `six_inherited` | 409 / 404 ns | 550 / 542 ns | 315 / 306 ns |
| `one_replaced` | 405 / 401 ns | 581 / 576 ns | 333 / 328 ns |
| `riot_match_ids` | — | 1,078 / 1,062 ns | 659 / 647 ns |

A chain of literal names is composed by the macro as before, so its cost is unchanged. A chain
with a native constant compares `HeaderName`s and classifies each native name (sensitive or
reserved list) when the request is built. Sending is unaffected: the loopback round trip
(`roundtrip/small`) takes about 8.1–8.6 µs, around 15 times the plan's cost. The trade-off was
accepted (F06): the alternatives either need const comparison of `HeaderName` (impossible on
stable) or read identifier spelling. Caching the per-name classification is a possible later
optimization (not done).

## 3. Compile time (hyperfine)

`benches/scale/ab.sh`, RUNS=3, 17 rows, three suites generated by the candidate's `gen.py`
(19:23–19:40Z): **base** is `625ac49` with its in-place format syntax. **stmt** is `3b5c5ed` with
representation statements and literal header names. **native** is `3b5c5ed` with every
`"x-..."` header key written as a native `HeaderName` constant (`--keys native`). Seconds, mean ±
σ (compile-summary.md; artifact sizes in compile/sizes.tsv):

| row | base | stmt | native | stmt/base | native/base |
|---|---|---|---|---|---|
| check-scale1000 | 1.758 ± 0.012 | 1.803 ± 0.035 | 2.008 ± 0.023 | 1.03 | 1.14 |
| dev-scale1000 | 1.985 ± 0.048 | 2.045 ± 0.152 | 2.235 ± 0.019 | 1.03 | 1.13 |
| rel-scale1000 | 6.619 ± 0.081 | 6.662 ± 0.041 | 8.098 ± 0.036 | 1.01 | 1.22 |
| dev-editapi-use100 | 2.121 ± 0.036 | 2.191 ± 0.089 | 2.626 ± 0.221 | 1.03 | 1.24 |
| rel-editapi-use5 | 6.921 ± 0.009 | 7.003 ± 0.063 | 8.285 ± 0.043 | 1.01 | 1.20 |
| rel-editapi-use100 | 9.095 ± 0.028 | 9.077 ± 0.015 | 10.975 ± 0.058 | 1.00 | 1.21 |
| rel-editapi-fan_app | 9.089 ± 0.021 | 9.146 ± 0.040 | 11.008 ± 0.034 | 1.01 | 1.21 |
| rel-editconsumer-use5 | 0.381 ± 0.005 | 0.378 ± 0.012 | 0.409 ± 0.013 | 0.99 | 1.07 |
| rel-editconsumer-use100 | 2.555 ± 0.010 | 2.580 ± 0.012 | 3.134 ± 0.039 | 1.01 | 1.23 |
| rel-editconsumer-fan_app | 2.584 ± 0.020 | 2.565 ± 0.009 | 3.134 ± 0.039 | 0.99 | 1.21 |
| check-scale_deep | 0.277 ± 0.009 | 0.285 ± 0.007 | 0.899 ± 0.005 | 1.03 | 3.24 |
| rel-scale_deep | 1.035 ± 0.031 | 1.037 ± 0.022 | 11.459 ± 0.085 | 1.00 | 11.07 |
| check-scale_shared | 0.723 ± 0.004 | 0.773 ± 0.022 | 2.919 ± 0.023 | 1.07 | 4.03 |
| rel-scale_shared | 3.452 ± 0.007 | 3.494 ± 0.025 | 11.775 ± 0.031 | 1.01 | 3.41 |
| check-scale_typed | 0.251 ± 0.006 | 0.261 ± 0.012 | 0.273 ± 0.004 | 1.04 | 1.09 |
| rel-scale_typed | 0.932 ± 0.008 | 0.938 ± 0.005 | 1.088 ± 0.019 | 1.01 | 1.17 |
| check-scale_fanout | 0.177 ± 0.004 | 0.193 ± 0.012 | 0.185 ± 0.009 | 1.09 | 1.04 |

- **Representation statements cost nothing measurable**: stmt/base is 0.99–1.07 on every row of a
  second or more, and the generated artifacts are within 0.3 % of the baseline.
- **Native header names were superlinear on long chains**: 3–4× (check) and 3.4–11× (release) on
  `scale_deep` (32 nested scopes, each with a native key) and `scale_shared`. `libscale_deep.rlib`
  grew from 7.3 MB to 100 MB. Cause (finding P1): every value sent under a native key carried a
  run-time sensitivity condition comparing its key with every other native key on its chains, so
  the expansion grew quadratically (34,816 inline comparisons for `scale_deep`). Fixed in
  `0e56a65` (F09 amended: only non-implied conditions; 1,088 comparisons; expansion 7.7 →
  2.1 MB). The rows above are the measurement **before** the fix; the frozen candidate is
  re-measured in §4.
- On flat APIs (`scale1000`, the edit loops), native keys cost 13–24 % of compile time: the
  run-time header plan, key accessor functions and per-value conditions are generated per
  endpoint. Literal names, the default in every guide, cost nothing.

## 4. The frozen candidate (`d884461`)

The F09 amendments change only generated code for chains with a native key; they were measured
twice, in the same way as §1–§3 (`/home/canarit/projects/concord_v4_finish_work/tools/perf_final.sh`,
idle machine, same rows, RUNS=3, same CRITERION_HOME so the earlier baselines compare):
`d3e26e4` (pruned conditions, 20:03–20:35Z, raw data final-20261102T200329Z) and `d884461` (also
one cached function per value, `cfg`-gated; 20:41–21:14Z, raw data final-20261102T204126Z, both
under `/home/canarit/projects/concord_v4_finish_work/evidence/perf/`). The code changed again
after the final reviews; the final candidate is measured in §5.

**Allocations.** Identical to `625ac49` on all 84 shared IDs and to `389392f` on all 92 (both runs).

**Runtime** (`keys/` and `headers/` only, the paths the amendments touch; `d884461` against
`3b5c5ed` run 2): every `prepare` within −3 % to +2 %, for example `keys/six_inherited/concord_native`
541.9 → 551.8 ns, `keys/one_replaced/concord_native` 576.1 → 582.6 ns,
`keys/riot_match_ids/concord` 1,062 → 1,072 ns, `headers/16/concord_native` 733.4 → 710.8 ns. The
native-name premium of §2 (about 150–190 ns over literal names on these chains) is unchanged.

**Compile time**, native/base (stmt/base stays 0.99–1.06 on every row):

| row | `3b5c5ed` (§3) | `d3e26e4` | `d884461` |
|---|---|---|---|
| check-scale_deep | 3.24 | 1.51 | 1.47 |
| rel-scale_deep | 11.07 | 1.53 | 1.50 |
| check-scale_shared | 4.03 | 4.11 | 3.76 |
| rel-scale_shared | 3.41 | 2.64 | 1.96 |
| check-scale1000 | 1.14 | 1.17 | 1.17 |
| rel-scale1000 | 1.22 | 1.21 | 1.21 |
| rel-editapi-use100 | 1.21 | 1.21 | 1.20 |
| rel-editconsumer-use100 | 1.23 | 1.22 | 1.15 |

`libscale_deep.rlib` (native): 100.4 MB → 11.1 MB → 8.4 MB; `libscale_shared.rlib`: 116.4 →
78.3 → 35.2 MB (literal names: 7.3 and 30.4 MB). What remains is linear: an endpoint whose chain
has a native key gets its own header plan, one binding, one entry and one guarded send per
contribution on the chain (`scale_shared`: 34 native contributions on each of 200 endpoints;
type-checking it takes 3.8× the literal-name time). Literal names, the default in every guide,
cost the same as the baseline.

## 5. The final candidate (`81836ec`)

The reviews changed generated code for chains with native keys:
- one key per path occurrence (X-R1);
- conditions computed on every chain (O-R1);
- slot flags over the whole history (O-P2);
- complete header-operation lists (N1);
- a fixpoint that visits each pair of contributions once (N3).

An intermediate commit, `0f559d8`, was not timed: the semantic re-review measured its
`macro_expand_crate` on the shared native suite at 4.8 s, against 2.3 s for `d884461`. That is the
regression N3 fixed. The final candidate was measured in the same way as §4, on an idle machine,
01:28–02:01Z on 2026-11-03 (raw data final-20261103T012851Z, same directory; baseline `final3` in
the shared CRITERION_HOME).

**Allocations.** Identical to `625ac49` on all 84 shared IDs and to `389392f` on all 92.

**Runtime** (`keys/` and `headers/`, against `3b5c5ed` run 2): every `prepare` within −4 % to
+2 %; `keys/six_inherited/concord_native` 541.9 → 550.9 ns, `keys/one_replaced/concord_native`
576.1 → 583.6 ns, `keys/riot_match_ids/concord` 1,062 → 1,073 ns.

**Compile time**, native/base (stmt/base on the same rows: 0.93–1.05, except
`dev-scale1000` at 1.14, whose stmt and native runs both have σ ≈ 0.2 s):

| row | `3b5c5ed` (§3) | `d884461` (§4) | `81836ec` |
|---|---|---|---|
| check-scale_deep | 3.24 | 1.47 | 1.30 |
| rel-scale_deep | 11.07 | 1.50 | 1.47 |
| check-scale_shared | 4.03 | 3.76 | 2.29 |
| rel-scale_shared | 3.41 | 1.96 | 1.86 |
| check-scale1000 | 1.14 | 1.17 | 1.18 |
| rel-scale1000 | 1.22 | 1.21 | 1.21 |
| rel-editapi-use100 | 1.21 | 1.20 | 1.21 |
| rel-editconsumer-use100 | 1.23 | 1.15 | 1.17 |

Native-key rlibs: `libscale_deep` 8.4 MB (literal names 7.3 MB), `libscale_shared` 35.4 MB
(30.4 MB), `libscale1000` 40.9 MB (36.7 MB). The remaining premium is linear in the native
contributions on each endpoint's chain: one header plan, binding, entry and guarded send each.
`scale_shared` puts 34 native contributions on each of 200 endpoints and type-checks in 2.3× the
literal-name time. On a flat API every header key being native costs 17–21 % of compile time.

## Not measured

HTTP/2 and TLS; Windows/macOS; rust-analyzer; allocation counts in round trips; profiles (`perf`
unavailable: `perf_event_paranoid = 4`, a system setting this campaign does not change).
