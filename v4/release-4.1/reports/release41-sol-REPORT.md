Two **should-fix** findings remain. I found no blocking or minor issue in the closure delta.

1. **Should fix: the Windows argument guard gives false passes and false failures.**  
   [scripts/argv-guard.sh:8](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/scripts/argv-guard.sh:8) counts every backslash twice and counts non-BMP Unicode characters once, although Windows counts UTF-16 code units. Its claimed “lower bound” is incorrect.

   Reproducer: `python3 audit/review/guard_probe.py` ([source](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/guard_probe.py)).

   | Argument | Modelled Windows length, including NUL | Observed guard exit |
   |---|---:|---:|
   | 40,000 ASCII characters | 40,011 | 126 |
   | 17,000 backslashes | 17,011 | 126 — false failure |
   | 17,000 non-BMP characters | 34,011 | 0 — false pass |

   Exact false-failure output:

   ```text
   argv-guard: a 34012-character command line for tool exceeds the Windows limit of 32,767
   ```

   The Unicode case produced no output and exited successfully. These were personally executed Linux probes; Windows lengths were calculated using Python’s Windows argument quoting and UTF-16 encoding. The guard catches the original bulk-data regression, but the changelog overstates its general enforcement.

2. **Should fix: several new network reads have no timeout.**  
   The raw request at [gzip.rs:114](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/checks/consumers/facade-gzip/tests/gzip.rs:114) is unbounded. The same applies at lines 168, 185 and 188. The new Accept test also has unbounded response reads and task joins after its initial timed assertion.

   Reproducer: `python3 audit/review/reproduce_unbounded.py` ([source](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/reproduce_unbounded.py)). It changes only the final raw-response fixture to withhold its body, in an audit-only source copy.

   Exact observed output:

   ```text
   running 1 test
   AUDIT: external 15s deadline expired (test did not terminate).
   ```

   The harness reported `external_exit=124`; the test’s five-second `LIMIT` never ended this read. After restoration:

   ```text
   test gzip_json_is_decoded_from_the_unpacked_body ... ok
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.03s
   ```

   Bound every request, retained-response read and join, or bound each complete test.

**Execution settings were verified.** The current thread’s recorded effective settings are `model: gpt-6.1-sol`, `effort: max`, approval policy `never`. These override the different global defaults. Evidence is saved in [settings.json](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/settings.json).

I reviewed snapshot HEAD `f51efb95deccf5b1d6a8c92dbea6049f72bf16e4` against the supplied `31984ae..f4a8324` delta. All 24 supplied patch postimage hashes match. The delta changes no production `src/` file and no historical `v4/` report: syntax, parser/codec design, architecture and public library API remain unchanged.

**Original Windows failure — inspected logs, then personally reproduced under the guard.** Run `41635670821`, Windows job `125916104134`, contains exactly one failed internal gate:

```text
FAIL  consumers  (see target/verify-consumers.log)
scripts/consumers.sh: line 53: /c/hostedtoolcache/windows/Python/3.12.10/x64/python3: Argument list too long
```

Benchmarks and package gates were omitted by that older Windows workflow; they did not pass there. Linux job `125916104643` passed all 34 gates.

Running the exact supplied original `consumers.sh` under the candidate guard in an audit-only candidate copy produced:

```text
exit=126
argv-guard: a 65818-character command line for python3 exceeds the Windows limit of 32,767
```

Reproducer: [reproduce_original.py](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/reproduce_original.py).

The root cause and metadata-file fix are correct and localized. Inspection of `scripts/` and `tests-gen/` found no remaining bulk-data argument: the synthetic SDK formatter already receives its generated text through stdin. The later Windows logs independently substantiate both follow-up fixes:

- `15b3200`: `package-check` and `consumers-packaged` failed with `tar (child): Cannot connect to D: resolve failed`; stdin archives address this.
- `791e991`: only `package-check` failed with the carriage-return-split archive path; trimming the trailing CR addresses this.

**Focused tests and mutations were personally executed.** Direct candidate runs passed all four Accept tests and all four gzip tests. The following mutations were made only under `audit/review/tree`; every mutant returned exit **101**, with **0 passed, 1 failed**.

| New test | Mutation | Observed failure |
|---|---|---|
| `no_suggestion_keeps_the_metadata_checks` | Restore the old wrapper | `refused without waiting for the body: Elapsed(())` |
| Same test | Read a body chunk before metadata checks | Same timeout failure |
| `gzip_json_is_decoded_from_the_unpacked_body` | Disable decompression | `the body is not valid JSON (line 1, column 1)` |
| `an_incompatible_type_is_refused_before_the_body` | Read a body chunk before metadata checks | `refused without waiting for the body: Elapsed(())` |
| `decompression_disabled_control` | Remove `.no_gzip()` | `test decompression_disabled_control ... FAILED` |
| `a_gzip_media_type_is_not_a_content_encoding` | Make JSON accept `application/gzip` | `left: Decode` / `right: Metadata` |

The restored gzip suite passed four tests; the restored Accept regression passed. The final audit source copy matches the candidate byte for byte. Reproducer and logs: [run_focused.py](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/run_focused.py), [focused-run.log](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/focused-run.log).

The wrapper forwards `check_content_type`, `context` and `decode_body`, leaving `ACCEPT = None`. Section 8 correctly explains that reqwest’s own `Accept` can still reach the wire. The mutations demonstrate that the new test detects both the old wrapper and premature body consumption.

The gzip fixture independently round-tripped exactly: **378 compressed bytes → 3,773 bytes of JSON**, containing 60 items. Tests cover decoding, the format-visible decompressed head, retained-response reading, disabled decompression, and the distinction between encoding and media type. The withholding assertions establish body-read ordering through server body writes. The timeout finding above remains.

I checked every changelog claim group against code or tests:

| Claim group | Evidence |
|---|---|
| Crate roles, features and format paths | Manifests, exports, macro path parsing, graph and format tests |
| Strict JSON and `IgnoreContentType` | JSON/media implementation and media/metadata tests |
| Accept fallback and unread metadata errors | Accept tests, decoder ordering and mutations above |
| `validate_header_names()` and earlier failures | Generated implementation and `validate_names` tests |
| Decompression belongs to reqwest | Gzip consumer and passing feature-isolation gate |
| Versions and toolchain | Four packages at 4.1.0; four normal intercrate edges exactly `=4.1.0`; both lockfiles consistent; minimum and pin 1.99.1 |
| Windows verification claims | Native logs support the inventory; guard enforcement needs qualification as finding 1 |

README status and migration guidance are updated. Remaining alpha-version mentions describe historical migrations. Package inventories matched the allowlist, excluded test fixtures and keys, and passed the credential-pattern scan. Package consumers passed two default-feature tests and one no-default-feature test.

**The full gate was personally executed once**, using the supplied `CARGO_HOME`, an initially absent `audit/target`, and offline Cargo:

```bash
CARGO_TARGET_DIR="$PWD/audit/target" CARGO_NET_OFFLINE=true \
  bash scripts/verify.sh --package --bench
```

Exit **0**. Every summary PASS/FAIL line was:

```text
PASS  toolchain
PASS  ci-inventory
PASS  graph
PASS  fmt
PASS  records
PASS  generated
PASS  clippy
PASS  test
PASS  test-no-default
PASS  test-json-only
PASS  test-reqwest-defaults-only
PASS  test-form-only
PASS  test-form-json
PASS  test-extras-only
PASS  test-core
PASS  test-extras-none
PASS  test-extras-json
PASS  test-extras-form
PASS  clippy-features
PASS  consumers
PASS  feature-isolation
PASS  test-multipart-alone
PASS  doc
PASS  doc-no-default
PASS  doc-json-only
PASS  doc-form-only
PASS  doc-form
PASS  doc-extras-only
PASS  doc-core
PASS  doc-extras
PASS  doc-extras-none
PASS  bench-build
PASS  package-check
PASS  consumers-packaged
```

The nested package log additionally reported:

```text
PASS  package-check (consumer built against the extracted .crate files)
```

Workspace tests reported **651 passed, 0 failed**. Both consumer passes included gzip’s four tests. All dependencies were available offline; no download workaround was needed. Full output: [verify.log](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/verify.log).

**Final native CI evidence was inspected, not personally executed.** Supplied run `41650716219` names source SHA `f4a8324a8c5f91964af7510bd5853148a5f0c842`.

| Native job | Job ID | Internal result |
|---|---:|---|
| Linux `verify` | `125967789174` | 34 PASS, 0 FAIL |
| Windows `verify` | `125967789323` | 34 PASS, 0 FAIL |
| Linux `nightly-clone` | `125967788668` | PASS; 16 tests |

Both verification jobs ran the same complete inventory, including benchmarks and packages. I verified all supplied CI SHA256SUMS. Details: [ci-summary.txt](/home/canarit/projects/concord_v4_modular_work/snapshots/release41-sol/audit/review/ci-summary.txt).

I did **not** personally run native Windows, nightly, timed benchmarks, the complete `31984ae` workspace, or brotli/zstd/deflate runtime consumers. I did not query live GitHub, crates.io publication status or remote tags. Tracked files remain unchanged; no push, publication or global-setting modification occurred.