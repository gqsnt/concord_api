Two **should-fix** findings remain, plus one **minor** documentation mismatch. The exact original X-F1 and X-F2 reproducers now pass. No blocking finding was established in this targeted review.

1. **Should fix — Text bypasses readable foreign or ambiguous charsets when `mime` rejects valid parameter syntax.**

   [text.rs:33](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/crates/concord-extras/src/text.rs:33) runs the scanner only after `media::single()` succeeds. Consequently, `mime`’s parameter-parsing limitations suppress the scanner’s refusal.

   Exact observed output (`\t` represents a tab on the wire):

   ```text
   CONTRACT Text input="text/plain;\tcharset=iso-8859-1" scanner=Some(["iso-8859-1"]) actual=ACCEPT
   CONTRACT Text input="text/plain; charset=utf-8;\tCHARSET=iso-8859-1" scanner=Some(["utf-8", "iso-8859-1"]) actual=ACCEPT
   CONTRACT Text input="text/plain; charset=\"\"" scanner=Some([""]) actual=ACCEPT
   ```

   For the first case, `mime` alone reports:

   ```text
   mime parse error: an invalid token was encountered, 9 at position 11
   ```

   The scanner correctly extracts the foreign charset. Text nevertheless decodes the body successfully. Escaped quotes and empty quoted values in another parameter provide additional reproductions.

2. **Should fix — Json and Form still refuse valid UTF-8 metadata.**

   [media.rs:23](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/crates/concord-extras/src/media.rs:23) parses the entire field with `mime`, including its parameters, before invoking the scanner.

   Exact observed output:

   ```text
   CONTRACT Json input="application/json;\tcharset=utf-8" scanner=Some(["utf-8"]) actual=Metadata: the response Content-Type is not a media type
   CONTRACT Form input="application/x-www-form-urlencoded;\tcharset=utf-8" scanner=Some(["utf-8"]) actual=Metadata: the response Content-Type is not a media type
   ```

   `mime` reports invalid token `9` at positions 17 and 34, respectively. The scanner accepts both. Other reproduced false refusals include empty parameters, whitespace before another semicolon, `audit=""`, and an escaped quote inside another parameter’s quoted value.

3. **Minor — Text’s public documentation omits the new duplicate-charset refusal.**

   [text.rs:13](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/crates/concord-extras/src/text.rs:13) describes “The one refusal” as a charset naming something other than UTF-8. Equal repeated UTF-8 parameters now also fail.

   Reproducer:

   ```text
   text/plain; charset=utf-8; charset=utf-8
   ```

   Exact observed decision:

   ```text
   wire=Metadata: the response Content-Type declares charset more than once
   ```

All three findings reproduce with:

```bash
CARGO_HOME="$(cat audit/results/cache-path.txt)" \
CARGO_TARGET_DIR="$PWD/audit/target" \
cargo test --offline --manifest-path audit/review-probe/Cargo.toml \
  -- --nocapture --test-threads=1
```

Result: **5 passed, 2 failed**. The two failing contract tests expose findings 1 and 2. [Probe source](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/audit/review-probe/tests/wire.rs), [exact output](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/audit/results/review-probe.log).

**X-F1:** the original quoted UTF-8-before-another-parameter case is resolved across Json, Text, and Form.  
**X-F2:** the original conflicting duplicate case is resolved across all three; equal and mixed-case duplicates are also refused when `mime` accepts the field. Finding 1 limits the broader Text guarantee.

I personally executed **58 cases × three formats = 174 loopback requests through `api!`**. The [complete case table](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/audit/results/matrix.md) records each input, `mime` result, scanner result, and each format’s decision. It covers all requested categories. Trailing field whitespace is distinguished from internal whitespace because HTTP strips field-boundary OWS.

No parser panic or hang occurred in the matrix or in **24,576 deterministic stress inputs**, **73,728 format checks**, and a **1,500,051-byte** scanner input.

The unread-body and privacy checks personally executed successfully:

```text
UNREAD refusals=12 bodies_recovered=12 leaked_markers=0
Error.source=None CodecError.source=None
```

Each metadata refusal returned while the server had written zero body bytes. After releasing the server, reading the retained response recovered the complete original body. All four error-rendering surfaces—`Display` and `Debug` for `Error` and `CodecError`—excluded the header marker. Static label types were checked during compilation; attached parser sources remained withheld.

`HeaderNameErrors` also passed the independent four-endpoint check:

```text
problems=4 repeated_order_checks=4 defaults_or_values_evaluated=0
```

`Display`, ordinary `Debug`, and pretty `Debug` contained the same ordered problem lines. Header values and defaults were absent. Exact renderings are preserved in the probe log.

The prior suite was personally rebuilt from an initially absent `audit/target`:

| Invocation | Library tests | Built-in tests | Failed |
|---|---:|---:|---:|
| `--features builtin` | 2 passed | 5 passed | 0 |
| Without the feature | 2 passed | 0 | 0 |

The supplied probe lacked `audit/support.rs`; I restored it byte-for-byte from the earlier `final-exec-sol` audit. Its provenance and hash are recorded in [source identity evidence](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/audit/results/source-identity.json).

The final offline gate used its default target and exited **0**:

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
PASS  package-check
PASS  consumers-packaged
```

Workspace count: **648 passed, 0 failed, 13 ignored**—594 unit/integration tests and 54 doctests. [Final gate output](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/audit/results/verify.log).

Earlier gate failures were triaged: the configured cache was missing and its directory was read-only; placing the replacement cache inside the workspace then caused six UI snapshot failures from `$WORKSPACE` versus `$CARGO` path normalization. Using a temporary cache outside the workspace resolved them without changing expectations or library code. [Every step’s status across all three runs](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/audit/results/gate-history.md).

Settings and source identity were **inspected evidence**: the matching session transcript records `gpt-6.1-sol`, `xhigh`, execute mode, workspace-write, and approval policy `never`. This verifies recorded session settings, not backend execution identity. [Settings evidence](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview-sol/audit/results/effective-settings.json). HEAD matches `0ea4f96…`; all 1,712 original tracked blobs match `4ce3d01`, with only `.concord-review.json` added by the snapshot.

I inspected the complete corrections patch and all three changed documentation passages. Those additions do not claim more than the code provides; their explanatory examples were not separately compiled. I did not rerun the historical `9f8939f` baseline, benchmarks, nightly checks, or remote-server tests.

Final tracked-file status is clean. Library code, tracked documentation, and global settings were unchanged; nothing was pushed or published.