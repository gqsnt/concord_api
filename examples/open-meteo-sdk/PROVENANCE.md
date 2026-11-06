# Sources and fixture provenance

Access date for every source and fixture derivation below: **2026-11-06**.
No forecast endpoint was called, no credentials were supplied, and no fixture
is a captured live response. Tests only execute against the loopback server.

## Official sources rechecked

| ID | Official source and section | Version used |
| --- | --- | --- |
| D | [Forecast documentation](https://open-meteo.com/en/docs): API Documentation, Hourly Parameter Definition, JSON Return Object, Errors; location/time and output controls | Rolling, unversioned website snapshot read 2026-11-06 |
| Y | [Requested OpenAPI source](https://github.com/open-meteo/open-meteo/blob/main/openapi/forecast.yml), pinned to [forecast.yml at 97132c51](https://github.com/open-meteo/open-meteo/blob/97132c51bbb138c0041ebbbe6570e4140943eb03/openapi/forecast.yml): `/v1/forecast`, GET parameters and response schemas | OpenAPI 3.1.0, `info.version: '1.0'`; repository commit `97132c51bbb138c0041ebbbe6570e4140943eb03`, dated 2026-11-04T10:29:08Z |
| C | [Official CSV writer](https://github.com/open-meteo/open-meteo/blob/97132c51bbb138c0041ebbbe6570e4140943eb03/Sources/App/Helper/Writer/CsvWriter.swift): `toCsvResponse` and `ApiSectionString.writeCsv` | Same repository commit as Y; location table, blank separator, `time` plus unit-bearing headers, numeric/`NaN` cells, `text/csv; charset=utf-8` and attachment header |
| A | [Maintainer answer in discussion #1052](https://github.com/open-meteo/open-meteo/discussions/1052): accepted answer by patrick-zippenfenig, 2024-10-08 | Dated answer confirming the `format=csv` query selector and attachment behavior |

The requested documentation and OpenAPI URL were both opened again for this
task. The website was readable through the web tool; a separate Python fetch
returned HTTP 403. GitHub raw downloads succeeded. GitHub's commits API resolved
`main` to the commit above; the pinned OpenAPI download matched the `main`
download byte for byte by SHA-256. No API response-format enum for CSV was found
in this OpenAPI file, so C and A are additional official evidence rather than
pretending that Y specifies CSV's whole wire layout.

Source fingerprints (downloaded official files, not forecast responses):

| File | SHA-256 |
| --- | --- |
| `openapi/forecast.yml` | `19cf8b43174a67d1964772e566fa7d438bb4d0a0d810a75a1f07a2c71d42a066` |
| `Sources/App/Helper/Writer/CsvWriter.swift` | `194b4ef1777cf8086c950f85d8767bc6dc38781ca5c749d84720bce26f71b49e` |
| `Sources/App/Helper/Writer/ForecastApiResult.swift` | `855ca669f75987cdb5a8886911cc94aa8d4e5193aaa4dd9a92687045fac352a3` |

## Fixture inventory

All files are under `fixtures/`. Source IDs refer to the official section,
version and access date above. All bytes were hand-authored for this task;
source-derived values/layouts and synthetic changes are distinguished here.
`.gitattributes` applies `fixtures/** -text` to JSON, CSV, invalid UTF-8 and
intentionally incomplete files alike.

| Fixture | Source → transformation / synthetic case |
| --- | --- |
| `forecast.json` | D, JSON Return Object; Y, 200 schema → manually transcribed envelope and first three hours, removed illustrative ellipses, added `hourly_units.time=iso8601` from the documented time format. Source example's latitude, longitude, elevation, generation time, offset and timezone labels retained. |
| `forecast.csv` | D's same three temperature observations + C's single-location/hourly layout → hand-authored equivalent two-table CSV, LF, UTF-8, one-decimal temperatures. This is a constructed CSV counterpart, not a downloaded vendor example. |
| `forecast-crlf.csv` | `forecast.csv` → every LF replaced with CRLF; synthetic checkout/reader portability case. |
| `forecast-extra.csv` | D, hourly wind-speed definition; C, unit-bearing headers and `NaN` → synthetic wind observations `5.4`, `NaN`, `7.2`; reorder hourly and location columns; quote timezone; add synthetic location and hourly `provider_note` fields with commas, escaped quotes, an embedded LF and an empty quoted string. Notes are not claimed provider fields. |
| `forecast-pressure.csv` | D, hourly surface-pressure definition; C, layout → add synthetic pressures `1001`, `1002`, `1003` hPa before `time`; preserve temperature as an unselected, caller-visible column. |
| `provider-error.json` | D, Errors; Y, 400 response → manually transcribe `error`/`reason` shape and misspelled variable from the website, normalize the illustrative line break/indentation in `reason` to one space; final LF added. |
| `malformed-error.json` | D, Errors → synthetic JSON cut off immediately after `reason:`; deliberately no terminal LF. Also used as malformed JSON-labelled success bytes. |
| `mismatched-arrays.json` | `forecast.json` → retain three timestamps but shorten temperature to one value; synthetic structural failure. |
| `missing-json-unit.json` | `forecast.json` → remove the selected variable's unit entry; synthetic structural failure. |
| `null-values.json` | `forecast.json` → replace the second temperature with JSON `null`; synthetic missing-value case. |
| `unknown-variable.json` | D, surface-pressure definition; `forecast.json` → add synthetic numeric hourly `surface_pressure` array and `hPa` unit; tests exercise both an unselected extra field and explicit selection by name. |
| `wrong-width.csv` | `forecast.csv` → append an extra cell to only the last row; synthetic late structural failure. |
| `missing-time.csv` | `forecast.csv` → rename `time` to `timestamp`; synthetic required-header failure. |
| `missing-variable.csv` | `forecast.csv` → replace the selected temperature header with a wind-speed header; synthetic selected-column failure. |
| `missing-unit.csv` | `forecast.csv` → remove temperature's unit suffix; synthetic unit-preservation failure. |
| `duplicate-header.csv` | `forecast.csv` → replace the temperature header with a second `time`; synthetic ambiguity. |
| `duplicate-variable.csv` | `forecast.csv` → add a second temperature header with a different unit; synthetic variable-name ambiguity, rejected before row mapping. |
| `invalid-number.csv` | `forecast.csv` → change the last selected value to `not-a-number`; synthetic late cell failure. |
| `unterminated-quote.csv` | `forecast.csv` → open a quote before the final value, remove its closing quote and terminal LF; synthetic quote-at-EOF case reproducing the frozen decoder's permissive behavior. |
| `missing-location.csv` | `forecast.csv` → remove the entire location table and separator; synthetic envelope failure. |
| `wrong-location-width.csv` | `forecast.csv` → remove the timezone-abbreviation cell from the metadata row; synthetic metadata width failure. |
| `non-utf8.csv` | `forecast.csv` → replace the first `12.7` byte sequence with one byte `FF`; synthetic invalid UTF-8 failure. |
| `maintenance-precipitation.csv` | D, hourly `precipitation` definition and default `mm` unit; C, two-table layout and unit-bearing headers → hand-authored four-hour synthetic forecast. Precipitation is first, time third, temperature last; numeric unknown pressure values retain trailing zeros. Synthetic precipitation cells are `0.00`, `NaN`, empty and `1.20`. Quoted `provider_note (text)` cells exercise commas, escaped quotes, an embedded LF and empty text; this note is not a provider field. LF and UTF-8 bytes. |
| `maintenance-precipitation.json` | D, JSON envelope and hourly variables → hand-authored JSON counterpart of the maintenance CSV's numeric arrays, with `null` for missing values. Four synthetic timestamps, GMT/zero offset and synthetic wind, pressure and precipitation observations; coordinates/elevation/generation time reused from `forecast.json`. No textual note in the numeric JSON arrays. |

For the maintenance change, D's hourly parameter definition and precipitation
unit default were rechecked on 2026-11-06. C was downloaded again at the same
pinned commit; its SHA-256 matched the fingerprint above. The web tool's raw
GitHub fetch returned a cache miss; a Python HTTPS download of that source file
succeeded. Neither request fetched a forecast. Reordered fields, extra columns,
empty numeric cells and the note text are synthetic compatibility probes, not
claims about the provider's current output order or observed responses.

## Synthetic transport cases and dependencies

Tests synthesize 400/429/502/503 statuses, request identifiers, retry hints,
missing/repeated/mismatched Content-Type, unsupported charset/header parameter,
single-byte HTTP chunks, a stalled body, connection hang-up, a complete CSV
prefix followed by missing HTTP chunk termination, oversized bodies filled
with spaces, an oversized CSV field, and excess repeated CSV records. These
are deterministic fault/limit cases, not observed provider behavior. The
generated CSV limit cases use the hand-authored location/header prefix and
repeated sample row; no generated CSV is embedded in the production SDK.

Runtime dependencies are the workspace Concord facade (JSON and CSV features)
and `serde` with derive. Concord already provides the mature `csv`/`csv-core`
parser; no second CSV parser or vendor SDK was added. Its raw-record route
handles generic framing and quoting; the SDK maps provider-specific tables.
Dev dependencies are the required local testserver and workspace Tokio.
`Cargo.lock` only gains this SDK package entry, with no new registry packages
or version updates. The single unmatched-quote guard is documented in F4 of
[FRICTION.md](FRICTION.md).

The Concord baseline is `0983a349`; `rustc --version` reported
`rustc 1.99.1 (58cf8f9c3 2026-10-13)`. All cargo invocations used the requested
five-job limit and shared `target-a` build directory. Requested runner settings
were gpt-6.1-sol / xhigh; no independent runtime model-setting verification was
available or claimed. Fixtures and offline passes are not vendor conformance.
