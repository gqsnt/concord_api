# Maintenance: Unicode key and signed range

The inventory fixture's selected key changed from `reports/2026/März summary.txt` to
`reports/2026/München 東京 résumé.txt`. Its object bytes and metadata are unchanged. The
workflow's conditional range changed from `12-16` (`March`) to `19-25` (`revenue`). This is a
change to example data and a caller's range, not a change to the S3 API contract.

- **Declaration:** no change. `src/api.rs` already declares the object key and optional Range
  for GET and HEAD.
- **Adapter:** no change. `src/client.rs` already forwards the range and retains it for checking
  a 206 response; `src/sigv4.rs` already converts the generated target to S3's encoding before
  signing. The existing native preparation/execution and path rewrite are documented in
  `FRICTION.md`, with a maintenance reproduction below.
- **Model:** no change. `ObjectKey`, `ByteRange`, the object head and XML models already carry
  the needed values. The XML decoder ignores unknown elements by matching expected children.
- **Tests and fixtures:** changed the first listing page, the fake object store and workflow
  expectations. Added unknown root and object elements to the second page. A loopback test runs
  three scenarios: an ASCII key and old range, the new key and old range, then the new key and
  new range. It checks both decoded pages, the unrelated HEAD, returned ranges and bytes, the
  exact canonical request, independent signature verification and changed signatures. Both page
  requests and the HEAD retain their canonical requests and signatures across the scenarios.
- **Docs:** the README illustrates the Unicode key and new range; `PROVENANCE.md` records the
  added hand-authored XML elements. Fixture byte preservation remains `fixtures/** -text` in
  `.gitattributes`.

The selected key has **two independent fixture-data descriptions** that had to agree: the XML
listing and the fake object's store key. The workflow assertions and the new wire test separately
spell the expected key and encoded target; the README illustrates it as well. The range is set
once in the existing workflow, with separate expected Content-Range and body assertions; the new
comparison test independently supplies the old and new ranges and expected canonical header.
These test expectations intentionally repeat the facts so a wrong request or response is visible.
No independently maintained production declaration, adapter or model needed editing. Thus the
change was local to example data, tests and documentation, though it touched several descriptions
of the same example key.

To locate the pieces, read the SDK README, `tests/workflows.rs`, `tests/protocol.rs` and
`tests/common/mod.rs`, the listing fixtures and provenance, then `src/api.rs`, `src/client.rs`,
`src/types.rs`, `src/xml.rs` and `src/sigv4.rs`; consulted the Concord repository README, caller
and author guides, and pagination README. The existing provider SDKs supplied no solution. The
first attempt was made before reading `FRICTION.md` or `FIRST_USE.md`; the targeted workflow tests
then passed. No generated-name or compile surprise occurred during this change.

Existing Concord friction still applies: generated operations cannot be sent directly because
SigV4 must sign the finished request, and a key spanning segments needs the native target rewrite.
Reproduce with `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-amazon-s3 --test workflows changed_key_and_range_are_signed_on_the_wire_without_affecting_other_calls`;
inspect `Signing::sign` and `canonicalize_target` for the working path. The test observes the
encoded target and verifies the signature; it does not by itself establish what would happen if
the rewrite were removed. No new workaround or frozen-library edit was needed.
