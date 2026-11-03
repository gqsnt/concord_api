One **should-fix** and three **minor** findings remain. No blocking finding was established. This is a triaged re-review report, not an acceptance claim.

1. **Should fix — Text refuses malformed fields with an empty subtype.**

   [Text’s rustdoc](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/crates/concord-extras/src/text.rs:12) and [CUSTOM_FORMATS §7](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/docs/CUSTOM_FORMATS.md:415) promise to accept malformed fields. However, `mime` accepts an empty subtype, and [media::single()](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/crates/concord-extras/src/media.rs:36) consequently applies the charset verdict.

   All eight reproduced contradictions are below. `\t` represents a tab; every row was expected to be accepted.

   ```text
   input="text/; charset=latin1"
   actual=Metadata: the text response declares a charset other than utf-8
   input="text/; charset=utf-8;charset=utf-8"
   actual=Metadata: the response Content-Type declares charset more than once
   input="application/; charset=latin1"
   actual=Metadata: the text response declares a charset other than utf-8
   input="application/; charset=utf-8;charset=utf-8"
   actual=Metadata: the response Content-Type declares charset more than once
   input="x/; charset=latin1"
   actual=Metadata: the text response declares a charset other than utf-8
   input="x/; charset=utf-8;charset=utf-8"
   actual=Metadata: the response Content-Type declares charset more than once
   input="application/ \t; charset=latin1"
   actual=Metadata: the text response declares a charset other than utf-8
   input="application/ \t; charset=utf-8;charset=utf-8"
   actual=Metadata: the response Content-Type declares charset more than once
   ```

   [Exact mismatch rows](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/documentation-mismatches.txt).

2. **Minor — Form’s documentation excludes its supported `us-ascii` charset.**

   [Form’s rustdoc](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/crates/concord-extras/src/form.rs:15) says “no `charset` other than UTF-8”; [the comparison table](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/docs/CUSTOM_FORMATS.md:417) says “UTF-8 or absent.”

   These four distinct tested inputs were accepted:

   ```text
   application/x-www-form-urlencoded; charset=US-ASCII
   application/x-www-form-urlencoded; charset="US-ASCII"; audit=x
   application/x-www-form-urlencoded;charset=us-ascii;
   application/x-www-form-urlencoded;charset=us-ascii
   ```

   Exact wire-test output for the last case:

   ```text
   ACCEPT_DEFAULT Form configured=application/vnd.acme.v2+json observed="accept: application/x-www-form-urlencoded" response_charset=us-ascii decoded=OK
   ```

   The implementation agrees with the supplied intended policy; the documentation needs correction.

3. **Minor — MIGRATION names the wrong Form `Accept`.**

   [MIGRATION:114](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/docs/MIGRATION.md:114) says typed Json **or Form** endpoints now send `application/json`. The wire output above shows Form sending `application/x-www-form-urlencoded`, replacing the configured client default.

4. **Minor — DIAGNOSTICS retains the unconditional header-error claim.**

   [DIAGNOSTICS:631](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/docs/DIAGNOSTICS.md:631) says a duplicate native header produces that error “on every call.” An earlier encoder failure wins:

   ```text
   ORDER body=POST Order.body: request not built: body: AUDIT_ENCODER_REFUSED
   ```

   The validator reported the duplicate header for this endpoint. The corrected core/generated rustdoc and AUTHOR_GUIDE correctly qualify the guarantee; this passage remains stale.

Reproduce findings 1–3 with:

```bash
CARGO_TARGET_DIR="$PWD/audit/target" cargo test --offline --locked \
  --manifest-path audit/rereview2-probe/Cargo.toml --test extended \
  -- --nocapture --test-threads=1
```

Result: **2 passed, 1 failed**, exit **101**, preserving the eight Text contract mismatches. Replace `extended` with `contracts` to reproduce finding 4 and the additional contract checks: **3 passed**. [Extended output](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/extended-probe.log), [contract output](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/contracts-probe.log).

The requested historical findings have these dispositions:

| Finding | Result |
|---|---|
| X-F1 | **Resolved:** quoted UTF-8 followed by another parameter accepted by all three formats. |
| X-F2 | **Resolved:** equal and unequal repeated charsets refused by all three on readable, well-formed fields. |
| RR-1 | **Resolved for its reported inputs:** Text now refuses tab-separated foreign/repeated charsets and `charset=""`. Finding 1 above is a separate malformed-essence case. |
| RR-2 | **Resolved:** valid UTF-8 OWS, empty parameters and quoted-value forms accepted by Json/Form. |
| RR-3 | **Resolved:** Text’s rustdoc and comparison table now mention repeated-charsets refusal. |

I personally executed the following:

- **Fresh build into initially absent `audit/target`.** The previous probe passed **7/7**, including **58 × 3 = 174 loopback matrix requests**. Its seven test bodies were unchanged. Only the diagnostic module path changed to an audit shim generated from current `media.rs`, exposing `charsets` and forwarding the former `charset_verdict` name to `check_charset`. [Exact shim diff](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/scanner-shim.patch).
- **231 additional loopback matrix requests through `api!`**, covering every requested category. The eight Text cases above were its functional contract mismatches; no Json decision contradicted its documentation. [Previous matrix](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/previous-wire-matrix.txt), [extended matrix](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/extended-wire-matrix.txt).
- **6,550 direct essence-split checks:** `accepted=100 non_json_accepted=0`. Semicolon and whitespace placement did not admit a non-JSON type through Json.
- **24,576 stress inputs, 73,728 format checks**, and a **1,500,051-byte** scanner input. No library panic or hang occurred. The failing contract assertion and expected UI-mutant failure are test-harness failures.
- **26 withheld-body refusals**, including the new malformed-Text cases: all retained responses recovered their full bodies. Header markers were absent from `Error`/`CodecError` `Display` and `Debug`; the previous probe also confirmed both `source()` methods returned `None`.
- Decode-order checks confirmed an early context refusal yields `Metadata` with the unread response, while a refusal using context in `decode_body` waits for the body and yields `Decode`.
- The independent `HeaderNameErrors` test confirmed ordered Display lines, ordinary/pretty Debug lists, and no default/value evaluation. The facade’s `CODEC_ERROR_CONTEXT` re-export compiled and equalled `3`.

The seal mutation discriminated as requested:

```text
SEAL baseline_exit=0
SEAL unsealed_exit=101
SEAL restored_exit=0
```

Removing only `: sealed::Sealed` produced:

```text
test tests/ui/native_key_sealed.rs ... error
Expected test case to fail to compile, but it succeeded.
```

The source was restored byte-for-byte, and the restored test passed. [Mutation evidence](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/seal-mutation.json), [exact mutant output](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/seal-unsealed.log).

I ran the gate offline with its default target:

```bash
env -u CARGO_TARGET_DIR CARGO_NET_OFFLINE=true scripts/verify.sh --package
```

It exited **0**, with every PASS/FAIL line below:

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

Workspace count: **650 passed, 0 failed, 13 ignored**—596 unit/integration tests and 54 passing doctests. [Gate output](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/verify.log).

I inspected the complete documentation diff and relevant surrounding contracts. Beyond the findings above, Decode’s revised step-order wording and the numeric-position/source-access qualifications match the code and executed checks. I also inspected the **unpacked** `concord-macros` package: `readme = "README.md"` is present, and its **507-byte README matches the source byte-for-byte**. [Package evidence](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/package-readme.json).

Settings and provenance are **inspected evidence**: the active session record reports **`gpt-6.1-sol`, `xhigh`, approval policy `never`**. This establishes recorded settings, not backend identity. HEAD matches `5a9ad442c077b8be362e5df28f1dac2a34a87675`. All **1,714 source blobs** match the snapshot manifest for `3fa14b7…`, and reconstruct its recorded tree; the original commit object is absent locally. [Settings](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/effective-settings.json), [source identity](/home/canarit/projects/concord_v4_modular_work/snapshots/rereview2-sol/audit/results/source-identity.json).

I did not rerun historical baselines, benchmarks, nightly checks, other platforms, or external-network tests. Two audit-probe development errors were corrected only under `audit/`; their failed-attempt logs are retained. Historical reports were inspected, not counted as fresh execution.

Final tracked-file diff is clean; only `audit/` is untracked. Nothing was pushed or published, no global settings were changed, and no sandbox bypass was used.