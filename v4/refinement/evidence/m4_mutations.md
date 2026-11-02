# M4 evidence: oracles and targeted mutation checks

Branch `refine-m4-tests`, created at `433abf9` (`concord-v4-refine`). Linux, rustc 1.97.1.
Model: Claude Opus 5.5 (`claude-opus-5-5`); requested effort "medium", not observable from inside the
session. All timings are noisy (concurrent builds by other tracks).

## Added tests

| File | Tests | What |
|---|---|---|
| `crates/concord/tests/d15_surfaces.rs` | 7 | D15 surfaces ported to current syntax, each with a positive control (TLS name mismatch: label in native chain, not in Concord output; S07 runtime base shown in Debug, not echoed by errors, native execution of the same request shows it; S09 custom encoder text trusted; JSON encode error content-free vs `body_source`; S08 classification; S03 remove+reintroduce sticky). `required-features = ["reqwest-defaults", "json"]` (tls.rs's feature plus `json` for `Json`). |
| `crates/concord/tests/oracle_composition.rs` (generated) | 6 fns, 1233 checks | Composition oracle, see below. |
| `crates/concord/tests/oracle_ui.rs` + `oracle_ui/*.rs` (generated) + `*.stderr` (blessed, reviewed) | 1 (6 cases) | Oracle-rejected chains must fail to compile. |
| `crates/concord/tests/oracle_sensitivity.rs` (generated) | 252 | Sensitivity oracle, see below. |
| `tests-gen/oracle_composition.py`, `tests-gen/oracle_sensitivity.py` | — | Generators (Python 3 stdlib; `python3 -I`; `--check` for freshness, `--stats`). |
| `tests-gen/m4_mutations.py` | — | Mutation driver (refuses any path without `/mut` or with local changes). |

## Composition oracle (SEMANTICS §5)

Domain: chain client → scope → endpoint; for query key `k` and, in the same shape, header `x-k`,
each owner has one of 12 kinds: none, introduce literal, introduce optional, append literal,
append optional, override literal, override optional `else inherit`, remove, remove+introduce,
introduce+remove (source order; `remove` acts first), direct optional placement, direct optional
placement + remove. Query key `a` (client, after direct placements) and `z` (scope, after `k`)
expose slot position. 12³ = 1728 chains; the model's static liveness rules accept 441 and reject
1287; 1233 run-time checks over every presence combination of the optional values, asserting the
exact prepared query string and the ordered `x-k` values.

Six rejected chains (introduce while live, direct placement while live, override without writer,
override-else-inherit without writer, remove without writer, introduce-then-remove in one owner)
are trybuild compile-fail cases; each blessed diagnostic names the reason the model gives.

**Discrepancies: none.** Every one of 1233 checks and all 6 rejections agree with the
implementation on the first run. Model choices that the contract leaves implicit and that the
implementation turned out to share (worth stating in SEMANTICS if not already obvious):
1. Liveness is declaration-time: an introduction from an absent optional value still makes the key
   live (a later `override`/`remove` is accepted; `"k" = ...` is rejected), and still fixes the slot
   position (`C_io/n/al` with `cv` absent → `a=1&k=ea&z=1`, not after `z`).
2. "Remove acts first" also rejects `"k" = "x"; remove "k";` in an owner that inherits no `k`.
3. A direct placement on the client precedes the client's block statements in the query
   (`C_dp` → `k=…&a=1&z=1`).

Compile time (noisy, `CARGO_INCREMENTAL=0`, one test crate rebuild): ~3.5 s for
`oracle_composition` (228 KB, 441 endpoints); ~6.6 s including the run for `oracle_sensitivity`.
`oracle_ui` (trybuild) took ~20 s when blessing.

## Sensitivity oracle (SEMANTICS §9)

Domain: one client value `v`, `#[sensitive]` or not, reused in a non-empty subset of {host label,
path segment, query key Q, header H}; Q ∈ {`q`, `Access_Token` (safety list after
lowercasing/`_`→`-`), `tokens` (near miss)}, H ∈ {`x-h`, `Authorization`, `x-token` (near miss)};
placements written on the client (`v`) or the endpoint (`client.v`). 252 cases (182 withheld, 70
shown). Every case asserts: raw value on the native request (host, path, query, header value)
— the wire positive control; `HeaderValue::is_sensitive`; Prepared Debug (`<redacted>` in the right
spot, or the raw value when nothing makes `v` sensitive); client Debug (`v: <redacted>` vs
`v: "…"`); endpoint Debug (positive control on a public value `w`, no `v` when withheld); public
`p=w` never redacted; Display/Debug of a build error caused by another value (`x-bad`). 120 cases
(client placements with Q or H) also check stickiness under a public `override` and under
`remove` + public reintroduction (public value on the wire, key still `<redacted>` and flagged).

**Discrepancies: none.**

Discrimination check (run once, then reverted): removing `authorization` from the model's safety
list made 24 of 252 tests fail.

## Mutation checks

Disposable checkout: `git worktree add --detach .../wt/mut-m4 HEAD` (at `9648d50`, later moved
to `b238e7b` for the re-run of `c`), removed afterwards with `git worktree remove`. Command:

```
CARGO_TARGET_DIR=<m4-tests>/target CARGO_BUILD_JOBS=6 \
  python3 -I tests-gen/m4_mutations.py /home/canarit/projects/concord_v4_refine_work/wt/mut-m4 [names]
```

The driver runs the listed tests on the unmutated checkout (baseline), applies the edit (exact
anchor, must match once), runs them again (mutant), `git checkout`s the file and runs them a
third time (restored).

| Mutation (production site) | Tests | Baseline | Mutant | Restored |
|---|---|---|---|---|
| a1 endpoint `prepare` re-evaluates initializers (emit.rs, prepare) | values | 15 pass | FAIL: v03_initializer_evaluation_points, v03_v09_initializer_order_and_clone_retention | 15 pass |
| a2 endpoint `Clone` re-evaluates initializers (emit.rs, endpoint_clone) | values | 15 pass | FAIL: v03_initializer_evaluation_points, v03_v09_…, v10_request_clone_follows_the_data | 15 pass |
| b `remove` keeps ancestor occurrences (model.rs, `s.slot.items.clear()` dropped) | oracle_composition, composition | 6 / 37 pass | FAIL: all 6 oracle fns; composition c08_suppressed_contributions_are_not_formatted, c10_authorization_is_an_ordinary_header, no_query_string_when_nothing_survives | pass |
| c override/remove replaces slot sensitivity (`|=` → `=`, model.rs) | oracle_sensitivity, d15_surfaces, sensitivity | pass | 1st run: **oracle_sensitivity survived** (no override in its domain); d15 s03, sensitivity s04 failed. After adding stickiness: oracle_sensitivity 88 failed + s03 + s04 | pass |
| d no-output response buffered (`if false && self.drain`, prepared.rs) | concord-heap-check | 1 pass | FAIL: unit_response_is_drained_not_buffered | 1 pass |
| e `Error::source()` returns transport error (error.rs) | d15_surfaces, tls, sensitivity | pass | FAIL: adv_1_tls_name_mismatch_positive_control; tls c4_r_008 ×2; adv_1_sensitive_host_hides_transport_source | pass |
| f `HeaderMap::append` instead of `try_append` (__private.rs) | native_limits | 3 pass | FAIL: a_body_content_type_that_does_not_fit…, a_full_header_map_is_a_build_error_not_a_panic | 3 pass |
| g `encode_segment` ignores `Display` error (__private.rs) | display_errors | 2 pass | FAIL: every_placement_reports_a_build_error | 2 pass |
| h `else inherit` fallback emits nothing (model.rs, fallback = empty) | oracle_composition, composition | pass | FAIL: all 6 oracle fns; d04_override_truth_table, q4_override_absent_keeps_inherited, q5_nested_overrides | pass |

All 9 mutants killed. One per-test gap was found and closed: the sensitivity oracle alone did not
detect (c); commit `b238e7b` adds the stickiness dimension, and the re-run shows 88 oracle failures.

## Rerun

```
WT=/home/canarit/projects/concord_v4_refine_work/wt/m4-tests
python3 -I $WT/tests-gen/oracle_composition.py --check
python3 -I $WT/tests-gen/oracle_sensitivity.py --check
cd $WT && CARGO_TARGET_DIR=$WT/target CARGO_BUILD_JOBS=6 cargo test --locked -p concord --all-features \
  --test d15_surfaces --test oracle_composition --test oracle_ui --test oracle_sensitivity
```
