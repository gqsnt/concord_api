# Salesforce SDK maintenance exercise (DX-01)

## Task statement

From `contracts/DATA_AND_VALIDATION.md` §6 (handoff directory) and
`v4/extensions-real-apis/MAINTENANCE_BRIEF.md`: a staged evolution of `examples/salesforce-sdk`
(`concord-example-salesforce-sdk`) by someone who did not write it. The change: **add one
selected field and switch an existing export to a non-comma dialect**. Evidence to keep:
metadata/schema propagation, header/column mapping, parser reuse, and the consumer changes needed
without hand-editing CSV logic. This is a deliberately staged change, not a claim that Salesforce
changed.

**Chosen change.**

- Field: `HasOptedOutOfEmail` (standard Contact boolean, label "Email Opt Out", not nillable),
  added to the SDK's Contact Bulk export (`csv::ContactExportRow` + `csv::CONTACT_EXPORT_FIELDS`)
  as `has_opted_out_of_email: bool`. Source: the Salesforce Object Reference "Contact" page.
  `developer.salesforce.com` answered `403` to a direct fetch on 2026-11-05 (the SDK author hit the
  same refusal; `PROVIDER.md`), so **this field's presence and type in API 68.0 were not checked
  against a retrieved page**. It is a long-standing standard field; that is background knowledge,
  not evidence from this run.
- Dialect: the export's job dialect switches from `COMMA` / `LF` to `TAB` / `CRLF`. In this SDK
  the dialect is a per-job runtime value (`bulk::BulkDialect`, sent in the job body, echoed and
  checked on every job document, then turned into the codec's runtime `CsvOptions` by
  `csv::SalesforceCsv`). The SDK owns no export-level dialect, so the "existing comma export" is
  the downstream consumer's configured export (`MailingExport::new` states `COMMA` / `LF`
  explicitly), and the switch is a consumer change. The SDK needed **no** change for it.

## Commits

| Stage | Commit | Subject |
|---|---|---|
| 1 | `32fa1136db5049d7ad6135a6ad4bff5e5ea902a6` | maintenance salesforce: frozen initial consumer |
| 2 | `b45fe6c3ac22a168b0a39a2b374b94d9e1a95360` | maintenance salesforce: SDK change — HasOptedOutOfEmail in the Contact export |
| 3 | `dad33569401ea34354f124191972c76dfaef0bb8` | maintenance salesforce: updated consumer |

The commit adding this file records the findings only (no code). `src/before.rs` and
`tests/before.rs` are byte-identical from stage 1 to the last commit
(`git diff 32fa1136 HEAD -- checks/maintenance-salesforce/src/before.rs checks/maintenance-salesforce/tests/before.rs`
is empty). Base: `0e14f15a` (branch `ext44/maint-salesforce`). Stage 1 also updated `Cargo.lock`
offline (`cargo metadata --offline`) for this crate's new dependencies.

## Results (Rust 1.99.1, `CARGO_BUILD_JOBS=6`, Linux x86_64, loopback only)

| Command | Baseline `0e14f15a` | Stage 1 | After stage 2 | Final (stage 3) |
|---|---|---|---|---|
| `cargo test --locked -p concord-check-maintenance-salesforce` | placeholder | before: 4 passed | before: 4 passed (source unchanged) | before: 4 passed, after: 3 passed |
| `cargo test --locked -p concord-example-salesforce-sdk` | 20 + 4 wire + 2 doctests | — | 20 + 4 + 2 passed (1 unit test adapted) | same |
| `cargo test --locked -p concord-example-salesforce-app` | 30 passed | — | 30 passed (**tests** adapted, no source edit) | same |
| `cargo clippy --locked -p <each of the three> --all-targets -- -D warnings`, `cargo fmt --all -- --check` | clean | clean | clean | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p <check crate> -p <sdk>` | — | — | — | clean |

No live Salesforce request was made (no org; offline by design). Raw outputs of every run above
were kept in the session scratchpad, not committed.

## Stage 1 — frozen initial consumer

Written before any SDK change, from the SDK's public documentation and exports only.

- `src/before.rs` (261 lines): a mailing export. `export_contacts` describes `Contact`
  (`ctx.api().contacts().describe().send()`, errors through `SfError::from_concord`), checks that
  every name of `CONTACT_EXPORT_FIELDS` exists in the org (`SObjectDescribe::field`) and refuses
  the export before any job otherwise (`MailingError::MissingFields`); builds the SOQL from the
  same list plus an `Email LIKE` literal (`Literal::like_suffix`) and a `LIMIT`; creates the job
  with `BulkQueryJob::create(ctx, &soql, export.dialect)`; polls with `wait_complete`; reads the
  `Sforce-Locator` chain with `ResultSets::<ContactExportRow>` and `concord_pagination::sequential`
  under a result-set budget; returns typed rows, described columns, set/byte counts, outcome and
  resume point, plus two summaries (`by_loyalty_tier`, `without_email`).
- `tests/before.rs` (530 lines, 4 tests) against a loopback org: the exact request sequence and
  targets (`results?maxRecords=2`, then `results?locator=c2V0-1&maxRecords=2`), `Authorization`
  per request, `Accept` headers, the exact job body; typed rows across two result sets with quoted
  commas, doubled quotes and an embedded newline in the comma dialect, empty → `None`; an org
  without `Loyalty_Tier__c` refused after its describe call (no job) while a second org with its
  own token completes concurrently; the result-set budget (`Outcome::Bounded`, `Resume::At`); a
  `401 INVALID_SESSION_ID` as `MailingError::Describe(SfError::Api)` whose renderings carry no
  token.

**Test design, disclosed because the stage 2 result depends on it.** The loopback org behaves as
the Bulk API 2.0 pages describe: result sets carry exactly the requested columns in the requested
order ("same order as you requested them", API 50.0+), every field quoted, in the job's delimiter
and line ending; its describe document lists 18 standard Contact fields (including
`HasOptedOutOfEmail`, `Title`, `Phone`, `DoNotCall`, ...) plus the custom field in org A; a field
without a stored value is empty, or `false` for a boolean. The job body assertion takes the select
list as `CONTACT_EXPORT_FIELDS.join(", ")` — the consumer's contract is "export the SDK's Contact
export" — and pins everything the consumer owns literally (object, filter literal, `LIMIT`,
`"columnDelimiter":"COMMA"`, `"lineEnding":"LF"`, operation, content type). I knew the category of
the staged change before writing stage 1 (the brief says it), and chose these fixture rules with
that knowledge. A frozen test in the app's style (a literal six-column SOQL and literal six-column
CSV bodies) would have failed at stage 2 exactly as the app's own tests did (below). The literal
bytes before and after are recorded instead (see "Wire bytes").

Documentation read: `v4/extensions-real-apis/MAINTENANCE_BRIEF.md`, `SDK_BRIEF.md`, contract §6
and §12; `examples/salesforce-sdk/PROVIDER.md` (the SDK has no README; `cat README.md` failed);
the rustdoc (read in source) of `lib.rs`, `api.rs`, `csv.rs`, `bulk.rs`, `bulk_query.rs`,
`model.rs`, `soql.rs`, `ids.rs`, `error.rs`, `context.rs`; `examples/salesforce-app/README.md`,
`src/sf_b.rs`, `src/sf_a.rs` (header), `src/bin/sf_b.rs` as usage examples;
`crates/concord-extras/src/csv/mod.rs` and `options.rs`; `crates/concord-pagination/src/sequential.rs`
and `types.rs`; `examples/testserver/src/lib.rs`. Concord's `docs/` mention CSV only as a custom
format or a `Csv` type path (`AUTHOR_GUIDE.md`, `RECIPES.md`, `CHEATSHEET.md`); the CSV module's
rustdoc answered the runtime-options question. Where the export schema lives and how the dialect
reaches the parser were answered by `csv.rs` and `bulk_query.rs` docs; one question was not
answerable offline (the field's documentation page, 403).

Counter-evidence: the consumer compiled with no error and all four tests passed on their first run.
One annotation was needed (`ResultSets::<ContactExportRow>`: the row type cannot be inferred from
the column list); the factory closure `|key| sets.fetch(key)` needed none; no cast, no native
escape (every request is a declared operation or an SDK wrapper).

## Stage 2 — the SDK change

`git diff --stat 32fa1136 b45fe6c3`:

```text
 .../salesforce-app/tests/fixtures/MANIFEST.json    |  4 ++--
 .../tests/sf_b_bulk_csv_round_trip.rs              | 14 ++++++-----
 examples/salesforce-app/tests/support/mod.rs       | 13 +++++++----
 examples/salesforce-sdk/PROVIDER.md                |  5 +++-
 examples/salesforce-sdk/src/csv.rs                 | 27 +++++++++++++++++-----
 5 files changed, 43 insertions(+), 20 deletions(-)
```

SDK source: **4 code lines** (the attribute, the field, the list's type, the new entry) and 2 doc
lines on the field, in two items of `csv.rs` that must agree.

```rust
// the row (2 doc lines + attribute + field), appended after Loyalty_Tier__c
/// `HasOptedOutOfEmail` (standard boolean, label "Email Opt Out"): the Contact does not
/// want e-mail.
#[serde(rename = "HasOptedOutOfEmail")]
pub has_opted_out_of_email: bool,
// the column list: the array length is part of its public type
-pub const CONTACT_EXPORT_FIELDS: [&str; 6] = [
+pub const CONTACT_EXPORT_FIELDS: [&str; 7] = [
     ..., "Loyalty_Tier__c",
+    "HasOptedOutOfEmail",
 ];
```

Row docs (3 lines) state the new select list and that the boolean is never empty;
`PROVIDER.md` SF-D5 ("empty fields are `None` in typed rows") gains the exception and the source
note. No wrapper, declaration (`api.rs`), dialect or reader code changed: `SalesforceCsv`,
`ResultSets`, `BulkQueryJob` and the app's `export_soql`/`PartRunner` derive everything from the
list. `bool` rather than `Option<bool>`: the field is not nillable, so an empty value is a protocol
surprise and fails its result set (`Decode`), instead of reading as "not opted out".

SDK tests: the unit test `csv::tests::results_are_read_with_the_job_dialect_and_exact_header`
spells a six-column body literally: 2 lines changed, 1 assertion and 1 case (empty boolean →
`Decode`) added.

### Failed approach first: the struct field alone

The first edit added only the struct field (the constant is a separate item further down).
**Everything compiled** — SDK, app, frozen consumer, all targets: nothing ties the struct to the
list. The failures came at run time (only the SDK's CSV unit tests and the frozen consumer were run
at this point): the SDK unit test with a literal body, and 3 of 4 frozen tests (the fourth, the
`401` test, never reads results and passed), each with this error (byte positions vary):

```text
called `Result::unwrap()` on an `Err` value: Results(Read(ReadError { kind: Decode,
operation: Some("bulk_query.job.results"), position: Position { records: 0, bytes: 156 },
codec: Some(CodecError { label: "a CSV record does not decode into the requested row type",
context: [("record", 1)], source: Some("<withheld>") }), source: None }))
```

The message names the record but not the field, and the cause is withheld. Finding it needed a
deliberate `err.codec().unwrap().source_unredacted()` (a temporary probe in the SDK test, reverted):
``CSV deserialize error: missing field `HasOptedOutOfEmail` ``. A missing field name is schema
metadata, not record content; withholding it costs the maintainer a probe.

### Did old call sites compile unchanged?

| Call site | Result |
|---|---|
| Frozen consumer (`src/before.rs`, iterates `CONTACT_EXPORT_FIELDS`, reads `ContactExportRow` fields) | **unchanged, compiled, 4/4 passed** — and changed its wire: it now selects and reads the new column (below) |
| `examples/salesforce-app` sources (`sf_b::export_soql`, `PartRunner`, the `sf_b` binary) | **unchanged, compiled**; the SF-B export now selects, parses, stages and combines seven columns |
| `examples/salesforce-app` tests | compiled; **11 of 14 SF-B tests failed** at run time. Where the message shows the underlying error (the sequential-chain test directly, three others inside the printed report) it is the exact header check: `ReadError { kind: Format, ... CodecError { label: "the CSV header does not match the expected columns", context: [("field", 7)] } }` — the right cause, at the right position; the rest are status/count assertions downstream of failed parts. With the fixtures at seven columns all 14 pass |
| SDK unit test with a literal body | compiled, failed the same way (`field 7`) |

App adaptation (tests only, required because they spell the six-column wire literally and copy
the list): `support/mod.rs` `Row = [String; 6]` → 7 and `HEADER: [&str; 6]` → 7 (a second copy of
`CONTACT_EXPORT_FIELDS`), a seventh generated value (`true` every fourth row); in
`sf_b_bulk_csv_round_trip.rs` one row assertion, two literal headers, one literal output row, the
literal SOQL, the 60,000-row generator's row format, and the width fault fixture (one more field,
so the case stays a width error rather than becoming a boolean decode error); two `MANIFEST.json`
parameter texts (the boolean; part size now about 6.2 MB, measured: 12,388,419 response bytes for
2 × 60,000 rows, 11,428,377 before).

### Compile breaks for other downstream styles (uncommitted probe)

None of these is in the frozen consumer. A probe test written against the stage 1 schema
(`tests/probe.rs`, deleted after the run) gave, verbatim:

```text
error[E0063]: missing field `has_opted_out_of_email` in initializer of `ContactExportRow`
 --> checks/maintenance-salesforce/tests/probe.rs:7:15
  |
7 |     let row = ContactExportRow {
  |               ^^^^^^^^^^^^^^^^ missing `has_opted_out_of_email`

error[E0027]: pattern does not mention field `has_opted_out_of_email`
  --> checks/maintenance-salesforce/tests/probe.rs:15:9
   |
15 |     let ContactExportRow { id, first_name, last_name, email, account_id, loyalty_tier } = row;
   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ missing field `has_opted_out_of_email`
   (three help: suggestions: include it, `has_opted_out_of_email: _`, or `..`)

error[E0308]: mismatched types
  --> checks/maintenance-salesforce/tests/probe.rs:16:30
   |
16 |     let columns: [&str; 6] = CONTACT_EXPORT_FIELDS;
   |                  ---------   ^^^^^^^^^^^^^^^^^^^^^ expected an array with a size of 6, found one with a size of 7
```

All three point at the right cause. `ContactExportRow` derives `Serialize` and `PartialEq` and has
public fields without `#[non_exhaustive]`, so a consumer that builds expected rows or re-encodes
its own rows breaks at compile time; the array length makes the column list's size part of the
SDK's public API.

### Struct order versus list order (uncommitted probe)

With the field placed after `email` in the struct but appended at the end of the list, everything
compiled again; reading still worked (named mapping; the frozen consumer passed 4/4) but the app's
shard writer failed at run time:
`Encode(CodecError { label: "the CSV row's field names are not the columns, in order (columns do
not reorder fields)", context: [("record", 1)] })`. The codec names the cause precisely; the
compiler cannot, because the order lives in two items. Reverted.

### Wire bytes

Frozen consumer, job body (printed by `tests/before.rs`):

```text
stage 1: {"operation":"query","query":"SELECT Id, FirstName, LastName, Email, AccountId, Loyalty_Tier__c FROM Contact WHERE Email LIKE '%@example.test' LIMIT 500","contentType":"CSV","columnDelimiter":"COMMA","lineEnding":"LF"}
stage 2: {"operation":"query","query":"SELECT Id, FirstName, LastName, Email, AccountId, Loyalty_Tier__c, HasOptedOutOfEmail FROM Contact WHERE Email LIKE '%@example.test' LIMIT 500","contentType":"CSV","columnDelimiter":"COMMA","lineEnding":"LF"}
```

and its first result set's header (loopback org): `"Id","FirstName",...,"Loyalty_Tier__c"\n`
then `...,"Loyalty_Tier__c","HasOptedOutOfEmail"\n`. There is **no omitted-field compatibility**:
the field is not optional in the request. Every consumer that derives its statement from the list
starts requesting the column without a source change. Against an org that has the field this
propagates consistently (statement, header check, typed row, writer columns). Inferred, not
tested: an integration user without field-level read access to `HasOptedOutOfEmail`, or a
recording-based fixture, sees a failed job or a header mismatch. A consumer that needs the stage 1
wire must copy the old row type and list; the SDK keeps no versioned schema.

## Stage 3 — the updated consumer

`git diff --stat b45fe6c3 dad33569`: `src/after.rs` +105, `src/lib.rs` +3, `tests/after.rs` +468.

- `src/after.rs`: `MAILING_DIALECT = BulkDialect::new(ColumnDelimiter::Tab, LineEnding::Crlf)`;
  `mailing_export(..)` is the frozen `MailingExport::new(..)` with that dialect;
  `mailing_list(..)` **calls the frozen `before::export_contacts` unchanged** and partitions the
  rows on `row.has_opted_out_of_email` and `row.email`; `MailingList::to_csv` writes the
  recipients with `SalesforceCsv::new(self.dialect).writer(&FieldColumns::new(CONTACT_EXPORT_FIELDS)?)?
  .to_vec(Rows::new(self.recipients.iter()))?` — header and rows in the job's dialect.
- `tests/after.rs` (3 tests; the loopback org copied from `tests/before.rs` so the frozen file stays
  frozen): the byte-exact job body with the new column, `"columnDelimiter":"TAB"`,
  `"lineEnding":"CRLF"`; typed rows parsed from TAB / CRLF bodies with an embedded tab, doubled
  quotes and an embedded CRLF; describe validation now covering the boolean; the opt-out rule; the
  mailing file byte for byte
  (`...\t"Plain\t""Tabbed"""\t...\tSilver, Plus\tfalse\r\n`: tab and quote quoted, comma not);
  a job document that states `COMMA`/`LF` for a `TAB`/`CRLF` job refused as
  `SfError::Protocol(DialectChanged)` on the creation document (no poll, no result set); and an
  empty opt-out value failing its result set as `ReadErrorKind::Decode` at `record 1`, `field 7`,
  with no record value or token in `Display`/`Debug`.

Consumer changes needed for the whole staged change: one dialect value, the new field's use (one
`if`), and an optional file writer. Describe validation, the select list, job creation, the job
document checks and the typed result reading needed **no** consumer edit.

### Where the dialect flows (no hand-edited CSV logic)

`MailingExport.dialect` → `BulkQueryJob::create(ctx, &soql, dialect)` → `CreateQueryJob::new`
serializes `columnDelimiter` / `lineEnding` → every job document is checked against it
(`BulkQueryJob::check`, `DialectChanged`, tested above) → `ResultSets::new(&job, ..)` builds
`SalesforceCsv::new(job.dialect()).options().columns(names)` = `CsvOptions::new(HeaderMode::Present,
delimiter.byte())` + the fixed Salesforce profile (quote, doubled quotes, no escape/comment, no trim,
`write_terminator` from the line ending, `text/csv` media) → `CsvReader::new` → `open_for::<T>`.
The writer path (`SalesforceCsv::writer`) uses the same options. Neither consumer contains a
delimiter byte, a split, a join or a quote rule; the stage 2 diff touched no CSV code. The only
hand-written CSV is each test file's independent Salesforce-style writer of loopback bodies, kept
apart from the SDK's encoder on purpose (the app's tests do the same).

Static versus runtime: the codec's static `Csv<HasHeader, D, P>` appears nowhere on this path.
With a per-job delimiter it cannot; the switch was a runtime value, and the job-document check is
what stops the runtime value and the server from silently disagreeing. A consumer whose file
format were a fixed contract (for example a static `Csv<HasHeader, Tab>`) would find no Salesforce
`CsvProfile` in the SDK and no mapping from `Delimiter<b'\t'>` to `ColumnDelimiter::Tab`; this
consumer did not need one because its file follows the job's dialect. Observation from reading,
not a measured friction.

Line endings on read: the TAB / CRLF bodies parsed; per SDK decision SF-D13 an LF body for a CRLF
job would also parse rather than be refused (the codec reader accepts `\n`, `\r\n`, `\r`). Not
re-tested here.

## Concepts the caller had to learn

Stage 1: `OrgContext`; id newtypes (`FieldName`, `SObjectName`, `MaxRecords`); the `Soql` builder
and `Literal::like_suffix`; `BulkDialect`/`ColumnDelimiter`/`LineEnding`; `BulkQueryJob::create`,
`wait_complete`, `JobWait`; `ResultSets` with `concord_pagination::{sequential, Budget, Resume,
Outcome}`; `SfError::from_concord(e).await` (an async conversion to read the error document); and
the **coupling** between `ContactExportRow` and `CONTACT_EXPORT_FIELDS`. The caller passes the
same list twice, into the statement and into `ResultSets::new`; a mismatch would only surface as a
run-time `Format` error. Stage 3: `FieldColumns`, `SalesforceCsv::writer`, the SDK's `CsvError`, and
`concord::csv::Rows` (a Concord type the SDK does not re-export, so the consumer names
`concord` with the `csv` feature itself).

## Other measurements

- **Compiler errors met** in the committed code: none at any stage. Verbatim errors above come from
  the probe; the run-time errors of attempt 1 and of the stage 2 tests are quoted verbatim.
- **Casts / annotations / adapters**: one turbofish (`ResultSets::<ContactExportRow>`); no casts;
  no adapter.
- **Native escapes**: none.
- **Duplicated code**: the export schema exists as a struct and a list in the SDK, copied again in
  the app's tests (`HEADER`, `Row`, literal SOQL and headers) — 4 places for one column. The REST
  `model::Contact` is a second Contact schema with another optional encoding (`Field<T>`) and did
  not get the field (the task concerns the export). The loopback org (~250 lines) is duplicated
  between `tests/before.rs` and `tests/after.rs` by design. `before.rs` parses the list into
  `FieldPath`s twice (my code, not the SDK's).
- **Hidden collection**: `ResultSets::fetch` collects each result set into `Page.items: Vec<T>`
  before returning it, bounded by `maxRecords` through the reader's `max_records` limit; without
  `maxRecords` a whole result set (its size is Salesforce's choice) is held. The consumers' own
  `Vec`s and `to_csv`'s in-memory file are explicit.
- **Metadata not preserved**: the stage 1 schema (no way to ask the SDK for the six-column export);
  `DescribedColumn` keeps type and custom flag but not `nillable` (my choice); the decode error's
  missing-field name (withheld, see attempt 1).
- **Timings**: not measured beyond the app's existing scale test, whose numbers vary run to run;
  not used as evidence.

## Counter-evidence (what was easy)

- The dialect switch cost zero SDK lines and one consumer value, and the SDK still caught a server
  that contradicted it (`DialectChanged`). The TAB / CRLF parse and write needed no CSV knowledge
  from the caller.
- One coherent edit (struct + list) propagated to the statement, the header validation, the typed
  row, the app's export, staging and combined file, and the frozen consumer's describe validation,
  with no source change outside `csv.rs`.
- Every run-time failure named its cause and position: `the CSV header does not match the expected
  columns` at `field 7`; `the CSV row's field names are not the columns, in order` for the order
  slip; `Decode` at `record 1, field 7` for a bad boolean, with no record value rendered. Compile
  errors for literal/pattern/array styles pointed at the right line.
- Both consumer commits (stages 1 and 3) compiled and passed on their first run (only `cargo fmt`
  reflowed lines).

## Proposals (not implemented; no library code was edited)

1. **One source of truth for a CSV row's columns** (Concord, `concord::csv`). Serde already
   carries a struct's renamed field names (`deserialize_struct(name, FIELDS, ..)`); a helper that
   records them would remove the struct/list coupling, the `[&str; N]` public type, attempt 1's
   silent compile and the order slip:

   ```rust
   // today (SDK): two items that must agree, checked only at run time
   pub struct ContactExportRow { /* ..., */ pub has_opted_out_of_email: bool }
   pub const CONTACT_EXPORT_FIELDS: [&str; 7] = [/* ..., */ "HasOptedOutOfEmail"];

   // proposed: the list is derived from the row type (plain structs; not for flatten/untagged)
   pub fn contact_export_fields() -> &'static [&'static str] {
       concord::csv::struct_columns::<ContactExportRow>()
   }
   ```

   Without a library change the SDK could get part of this with an associated
   `const COLUMNS` next to the struct and a unit test that compares it with a serialized row.
2. **Name a missing column in decode errors** (Concord). For serde's `missing field` the codec could
   report a schema error (`"the row type needs a column the header lacks"`, with the row field's
   position) instead of a generic decode error with a withheld source. Before: `context:
   [("record", 1)], source: Some("<withheld>")` plus `source_unredacted()` to learn the name.
   After: the position or name in the ordinary message, still no record value.
3. **Tie the statement to the reader** (SDK, optional). A `BulkQueryJob::create_export(ctx,
   filter, dialect)` / `ResultSets::for_export(&job)` pair taking the columns from one place would
   stop callers from passing the list twice. Not needed for correctness today.

## Comparison with `examples/salesforce-sdk/FRICTION.md` (read after stage 3)

- **F2** (runtime CSV dialects bypass typed outputs, intentional): agreed and confirmed from the
  maintainer side. That design is why the dialect switch was free for the SDK.
- **F6** (exact header matching via `columns`, positive): confirmed. It turned every stale
  six-column fixture into a precise `field 7` failure instead of a silent mis-mapping.
- **F5** (reader cannot enforce the job's line ending): not re-tested; consistent with what the
  stage 3 test can and cannot show.
- **F14** (no routing helper in the test server): the same pressure here. Each test file carries a
  loopback org of about 250 lines.
- **Not in FRICTION.md**: schema evolution of the export. The struct/list coupling with no compile
  check (attempt 1, order slip), the public `[&str; N]` length, the non-`#[non_exhaustive]`
  serializable row (compile breaks for literal/pattern consumers), the silent select-list change
  for every consumer (no omitted-field compatibility), the withheld missing-field cause, and the
  four copies of the column list in the app's tests. FRICTION.md was written from the author's
  position, where none of these comes up until the schema changes.

## Fixtures and limits

- Loopback only; no org, so live acceptance of the field and of TAB / CRLF jobs is `NOT_RUN`.
- The loopback org is synthetic (`schema_derived_synthetic` in the app's terms): Salesforce-like
  quoting, column order and job documents from the Bulk API 2.0 pages the SDK cites. It does not
  model field-level security or invalid-field job errors. Ids, hosts and tokens are synthetic.
- Independence: I did not write the SDK and read `FRICTION.md` only after stage 3. But I knew the
  staged change's category before writing stage 1, which shaped the frozen test's fixture rules
  (disclosed in stage 1). The probe and attempt 1 results are from uncommitted edits, reverted
  before the stage 2 commit.
- One exercise, one maintainer: no population-level usability claim.
