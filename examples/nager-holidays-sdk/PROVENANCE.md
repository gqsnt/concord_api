# SDK01 provenance

Contract checked on **2026-11-06**: **Nager.Holidays / Nager.Date Community WebApi v4**.
The public origin is `https://nagerholidays.com`; the pinned operation is
`GET /api/v4/Holidays/{CountryCode}/{Year}`. Both requested official pages were
read at kickoff. No live provider endpoint was requested, no credentials were used,
and no live response bytes were captured.

## Official sources

- [Holiday API](https://nagerholidays.com/api), accessed 2026-11-06:
  **Quick Start Example / Parameters** gives country before year; **Response Model**
  gives the v4 field names and six holiday type values; **JSON Preview** supplies a
  two-entry AT/2026 array, including explicit `subdivisionCodes: null` values.
- [Nager.Date README at main](https://github.com/nager/Nager.Date/blob/main/README.md),
  accessed 2026-11-06: **How can I use it? / REST API** shows the v4 US/2026 URL and
  a REST API requiring no license key; **Holiday types** lists the six classifications;
  **Release and Support Lifecycle** identifies Community WebApi v4 (release date
  2026-06-30) separately from v3; **Data precision** explains subdivision codes and
  explicitly distinguishes v3's `counties` field. This is a dated observation of the
  mutable `main` document, not a claim to have pinned a source-code release SHA.
- The README's linked [Scalar reference](https://nagerholidays.com/scalar/) was opened
  as documentation only. The browser extraction returned no text; a direct HTML fetch
  returned HTTP 403. The two required official pages above were readable. They do not
  specify an error-body schema. This SDK makes no claim about a documented v4 failure
  document; the invalid-request body below is deliberately synthetic.

The version pin is the v4 URL and `API_VERSION = "v4"`, not a NuGet package version,
a v3 alias, or a guarantee about subsequent server behavior. No supported-country
inventory or calendar correctness was researched or asserted.

## Fixture mapping

All paths below are relative to `fixtures/`. JSON whitespace is hand-authored with
LF endings and a final newline; no byte file is a captured provider HTTP response.
Only the AT preview's data values were actually obtained from a documented example.
The other cases were authored from the response model or as explicit negative cases.
All HTTP statuses and response headers in tests are local scripts, including ETag,
Retry-After, duplicate headers and `x-synthetic-*` metadata. These are not documented
provider header guarantees.

| Fixture | Official page/section, version, access date | Transformation / status |
| --- | --- | --- |
| `holidays-at-2026.json` | Holiday API → JSON Preview; v4; 2026-11-06 | Transcribed the displayed two-entry array and its values; normalized indentation to two spaces and LF. The preview is not a full-year fixture. |
| `holidays-at-2026-perturbed.json` | Same JSON Preview and Response Model; v4; 2026-11-06 | Synthetic perturbation of the transcribed array: added unknown `futureOptionalField` as an object and as null; retained explicit null subdivision lists and every known value. |
| `holidays-de-2027-synthetic.json` | Holiday API → Response Model; README → Holiday types / Data precision; v4; 2026-11-06 | Fully hand-authored DE/2027 entries, names and dates; covers all six types, national/regional flags and populated, null, empty and omitted subdivision lists. No German calendar claim. Omission acceptance is SDK tolerance. |
| `holidays-de-2027-perturbed-synthetic.json` | Holiday API → Response Model / JSON Preview; v4; re-read for maintenance on 2026-11-06 | Hand-authored variant of the synthetic DE fixture: changed the regional bank entry's populated list to null; added unknown `futureOptionalField` as an object on that entry and null on the following entry. Every other known value is unchanged. Neither the unknown field nor this regional/null combination is an obtained provider example; no calendar or live-response claim. |
| `holidays-future-type-synthetic.json` | Holiday API → Response Model; v4; 2026-11-06 | Fully synthetic valid leap-date text, Unicode name, future type string alongside Public, and null subdivision list. Unknown-type retention is SDK compatibility policy. |
| `empty-array.json` | Holiday API → top-level JSON array model; v4; 2026-11-06 | Synthetic zero-element array; no claim that any country/year returns it. |
| `invalid-request-synthetic.txt` | No documented failure shape on reviewed pages; scope anchored to Holiday API → Parameters; v4; 2026-11-06 | Invented plain text and scripted HTTP 400 for malformed country/year. Neither bytes nor exact provider validation behavior is claimed official. |
| `invalid-date-synthetic.json` | Holiday API → date field; v4; 2026-11-06 | Synthetic nonexistent 2027-02-29 tests generic date parsing, not holiday calculation. |
| `missing-required-field-synthetic.json` | Holiday API → nationalHoliday response field; v4; 2026-11-06 | Synthetic object omits `nationalHoliday` to verify required-field rejection. |
| `v3-shape-synthetic.json` | README → Data precision (v3 `counties` distinction); target remains v4; 2026-11-06 | Hand-authored legacy-shaped object with `global`/`counties`/`types`; deliberately lacks required v4 fields. Not a captured or independently verified v3 example. |
| `wrong-root-synthetic.json` | Holiday API → JSON Preview array root; v4; 2026-11-06 | Synthetic object wrapper instead of an array, to verify shape rejection. |
| `truncated-array-synthetic.json` | Holiday API → Response Model; v4; 2026-11-06 | Synthetic valid first object followed by a comma and EOF. Used both as finite malformed JSON and a chunk prefix followed by a dropped connection. |
| `proxy-error-synthetic.html` | No provider body source; target contract anchored to Holiday API → JSON response; v4; 2026-11-06 | Invented HTML proxy body. Scripted as 200 wrong media type and 503; no vendor error behavior claim. |

## Implementation sources and dependency choices

The workspace README, `docs/CHEATSHEET.md`, `docs/CALLER_GUIDE.md`, relevant
`docs/AUTHOR_GUIDE.md` response/path sections, `docs/RECIPES.md`, crate READMEs and
the generated public Concord rustdoc were consulted. Rustdoc was generated with
`cargo doc -p concord --no-deps --features csv,jsonl` using the required environment
prefix. `Received::head`, `operation`, `decode`, and `into_response` were inspected.

Existing provider SDK reading supplied implementation patterns: GitHub's declaration,
error and metadata approach, and Stripe's `send::send` use of `Prepared::receive()`
and pre-decode metadata capture. This SDK keeps only a concrete holiday send policy
(`KeepHead`; a `fetch()` method until the between-waves 2-3 integration);
it does not copy their generic send/pagination/context infrastructure. No private
Concord APIs, native HTTP execution, vendor SDK or handwritten operation labels are used.

- `concord`: public declaration macro, transport boundary and JSON codec through its facade.
- `serde` derive: v4 object decoding; custom HolidayType deserialization only maps
  provider strings and preserves future values.
- `chrono` 0.4.45, defaults disabled, `std` and `serde` only: mature generic date parsing
  as `NaiveDate`, without clock/timezone dependencies or SDK-written date arithmetic.
  [Chrono NaiveDate documentation](https://docs.rs/chrono/0.4.45/chrono/naive/struct.NaiveDate.html)
  was consulted on 2026-11-06. Chrono is the only new registry package in the lock;
  its `num-traits`/`autocfg` dependencies were already locked in this workspace.
- Dev-only `concord-example-testserver` by path and Tokio: deterministic loopback HTTP tests.

No pagination dependency is added because the declared response is a single array.
Fixture bytes are protected by this crate's `.gitattributes` (`fixtures/** -text`).
Passing tests establish local SDK behavior only, **not vendor conformance**.
