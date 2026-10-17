# V3 runtime and contract-diff follow-up review

## Scope and result

The reviewed baseline is `570eec460dee1d83828ae48e35396ae07652aabc` on
`concord-v3`. This pass inspected runtime interactions and corrected CSV
document-start handling with native before/after evidence. A follow-up on the
independent Rustdoc exercise also confirmed and corrected excessive endpoint
attribution in contract diffs.

The [native evidence packet](evidence/runtime/README.md) retains the original
logs and receipts, a portable command and checksum index, and patches and file
hashes identifying the reviewed before/after source states. Environment-blocked
attempts are retained separately and excluded from the defect controls.

No new defect was confirmed in the inspected redirect, protected-origin,
retry-admission, exact-request-body or managed-stream terminal paths. This is a
bounded code review, not a claim that every possible concurrent execution or
external provider behavior has been tested.

## CSV document-start handling

**Severity: P2 — silent application-data change and inconsistent input acceptance.**

`CsvDecoder::parse_record` constructed a fresh `csv::Reader` for every logical
record. The locked `csv` 1.4.0 parser uses `csv-core` 0.1.13, whose reader strips
a UTF-8 byte order mark on its first read. Recreating the reader reset that
document-start state for every record. As a result:

- `plain,one,1` followed by `U+FEFFsecond,two,2` returned `second` without its
  leading U+FEFF.
- In a typed integer column, `U+FEFF2` incorrectly decoded as the integer `2`.
- A legitimate document BOM before a quoted first header or value failed in
  Concord's strict framing layer before the underlying parser could recognize it.

The relevant upstream behavior is visible in the pinned
[csv-core reader source](https://docs.rs/crate/csv-core/0.1.13/source/src/reader.rs),
in `read_record`, `has_read` and `strip_utf8_bom`.

The correction gives the incremental decoder one document-start flag. It
recognizes and consumes at most one complete BOM at the actual beginning of
the response, waiting for split prefix bytes when necessary. A later U+FEFF
remains field data, including after ignored blank lines or after a second
consecutive marker. A truncated UTF-8 prefix at EOF still fails.

When a framed record itself starts with U+FEFF, a borrowed empty-line prefix
advances the fresh parser past its file-start handling. This requires no copy
of the record bytes, no new dependency and no public API change. Existing
framing, byte limits, error reduction and managed-stream ownership remain in
their existing code paths. The CSV Rustdoc now states the BOM policy explicitly.

## Native verification

The controls used Rust 1.97.1 and the locked workspace dependencies. The old
production decoder was tested with the new regressions before restoring the
correction. These were actual compiled Rust executions.

| Control | Native result |
| --- | --- |
| Old decoder, three new unit regressions | All three failed with the expected missing-character or invalid-framing result; command exit 101 |
| Old decoder, two new managed-loopback regressions | Both failed: the invalid integer was accepted, and the quoted header after a BOM was rejected; command exit 101 |
| Corrected decoder, selected CSV tests | 6 unit tests and 8 integration contracts passed; command exit 0 |
| Formatting of the two changed Rust files | Pinned rustfmt completed; command exit 0 |

The three unit regressions cover comma, semicolon and tab delimiters, header
and positional decoding, every two-chunk split in the selected fixtures,
one-byte delivery, a marker after a blank line, repeated markers and truncated
prefixes. The two new integration tests use the production managed response
path with a local HTTP server. They assert EOF or one correlated decode error,
the exact delivered-record count and a single application request.

The successful selected run also retained the existing CSV malformed-quote,
unterminated-tail and overdue pending-read checks. Its native log is
`csv-after.log`, SHA-256
`03daa806b5791b01dc7b8f5704a499165b76c708cdec4a9d5a5b07db01c00888`.
The discriminant logs are `csv-before.log` and `csv-before-streams-native.log`.
An earlier sandboxed loopback attempt was unable to open a local listener; it
was retained separately and was not counted as product-failure evidence.

Changed implementation and regressions:

- `crates/concord-extras/src/records/csv.rs`
- `crates/concord-extras/tests/contracts/record_streams.rs`

## Dependency-site changes attributed unrelated endpoints

**Severity: P2 — misleading review scope in contract check/diff output.**

The independent Rustdoc exercise added only `response Json;` to
`CatalogApi::regional`. Seven regional endpoint records changed their
representation provenance; the root `catalog.auth.login` endpoint record was
identical before and after. The public diff correctly reported ten structural
changes on the `Provenance` axis, but its `changed_endpoints` list named all eight
endpoints, including the untouched root endpoint.

The extra attribution came from `dependencies/Json/sites`. Adding the regional
default added one structural use of the same representation. The inspector
resolved every old and new use of `Json`, including unchanged root defaults,
and consequently named the entire client's endpoint tree. This contradicted
the structural meaning of dependency `sites` and the changed-endpoint output:
an added use did not change the dependency or its existing consumers.

For a dependency's `sites` field, the correction resolves only added or removed
site memberships. It compares the complete site sets, so an insertion into a
sorted list cannot attribute an unchanged site merely because its index moved.
A whole dependency addition/removal or another dependency-field change still
resolves all before/after uses. The report format, change count, classifications
and represented before/after values are preserved.

Two regressions use complete declarations expanded by the real `concord::api!`
macro. Adding/removing a repeated scope default must name exactly its two
endpoints. Moving that extra use between the scope and an explicit `ping`
representation, with an unchanged site count, must name only those three
endpoints. Both tests check both diff directions and the dependency rows as
well as the report's combined endpoint list.

Both regressions failed against the old inspector, which named all five fixture
endpoints. The corrected `cargo test -p concord-inspect --all-features --locked`
run passed all 19 unit and 18 integration tests. Native evidence:

- `inspect-sites-before.log`, exit 101, SHA-256
  `1dd8695d38a9ef494ab0e50686ca8c6067230d36c39511a49d081200f1f1adfc`.
- `inspect-sites-after.log`, exit 0, SHA-256
  `5082f3a021b7d44adacb1f4490ebea3f0ec7ebbd67ea414e6190ec9037fb35f5`.

Replaying the original Catalog snapshots through the corrected public CLI
reported exactly the seven regional endpoints. All ten change records, their
before/after facts and classifications, and the empty unresolved list matched
the original report once endpoint-attribution lists were excluded. The native
log is `inspect-sites-catalog-isolated.log`, exit 0, SHA-256
`9e435076ae4ad8b39ad9bca8a7dec1a5c268afa5bddd0880c829de7c4e840d32`.
This replay used a fresh build target after two runs in the reused shared target
failed to locate the compiled inspector crate; those build failures are retained
separately and are not counted as diff results.

Changed implementation and regressions:

- `crates/concord-inspect/src/diff.rs`
- `crates/concord-inspect/tests/generated_contract.rs`

## Other inspected runtime interactions

- **Redirects and credentials:** application and provider clients disable
  redirects; provider sends check HTTPS, TLS availability, URL credentials,
  the declared origin grant and the acquisition's remaining send allowance
  before body construction or handoff.
- **Policy retries:** classifier appetite is followed by effect, dispatch
  knowledge, replayability, attempt-count, server-minimum, operation-deadline
  and shared-budget gates. `Retry-After` is parsed once per observed head and
  anchored to that observation; overflow prevents a policy retry.
- **Request bodies:** exact positive-length streams retain their final chunk
  until producer EOF proves the length. The zero-length path keeps EOF
  validation; lower bounds, empty-chunk limits and producer failures remain
  terminal checks. This pass did not alter the early-response contract.
- **Response and record streams:** the owned stream checks cancellation and
  lease/idle limits before reading or delivering staged records, accounts for
  acquired nonempty chunks, releases its source at termination and emits one
  terminal error. Record framing remains independent of transport chunk
  boundaries and enforces net decoder progress.
- **Local rate limiting:** conjunctive quota evaluation remains under one
  lock, waiting releases that lock, committed admissions remain consumed,
  and keyed cooldown capacity pressure retains a finite shared barrier.

The runtime correction remains limited to CSV document-start handling; the
inspector correction addresses the separately reproduced attribution defect.
