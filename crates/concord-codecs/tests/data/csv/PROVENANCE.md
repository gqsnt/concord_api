# Provenance of the CSV seeds

Class: `schema_derived_synthetic` codec seeds. These fifteen files are copied byte for byte from the
4.4 extension handoff (`fixtures/csv/`, cases `C01`–`C15` of its `cases.json`, schema version 1,
status `synthetic_seed_oracles_not_Rust_verification`). They are newly authored synthetic format
fixtures: not provider captures, not live interoperability results, and they contain no personal or
production data.

Each file's SHA-256 was checked against the `sha256` the handoff records for its case when it was
copied (all fifteen equal). `tests/csv_fixtures.rs` asserts each file's exact bytes, then the
documented expected headers, records, mapped values and error categories on the finite route, the
incremental route, every single split point and representative multi-split partitions. The
handoff's own `tools/verify_fixtures.py` checks only these oracles with Python's parser; it does not
test this Rust implementation.

| Case | File | Options | Expected |
|---|---|---|---|
| C01 | `comma.csv` | header, `,` | `id,name`; `1,Lyon`; `2,Zoë` (CRLF) |
| C02 | `semicolon_multiline.csv` | header, `;` | `id;note`; a quoted `;` and LF inside a field; doubled quotes |
| C03 | `headerless.csv` | no header, `,` | `001,Alice`; `002,Bob` |
| C04 | `supplied_columns.csv` | no header, `;`, columns `id,name` | mapped `{id, name}` rows |
| C05 | `tab.csv` | header, tab | `1`, `東京` |
| C06 | `unit_separator.csv` | no header, `0x1f` | `1,hello`; `2,world` |
| C07 | `multiline_header.csv` | header, `,` | header `display\nname,id` (quoted newline); `A,B` quoted |
| C08 | `bom.csv` | header, `,`, BOM stripped | `id,name`; `1,Alice` |
| C09 | `header_only.csv` | header | zero records |
| C10 | `empty_headed.csv` (empty) | header | missing-header error |
| C11 | `empty_headerless.csv` (empty) | no header | zero records |
| C12 | `duplicate_header.csv` | header, named | duplicate-header error |
| C13 | `width_error.csv` | header | width error after the prefix `1,Alice` |
| C14 | `invalid_utf8.csv` | header, typed | invalid UTF-8 error |
| C15 | `whitespace_and_hash.csv` | header, no comment byte | `  value  ` kept; `#ordinary data` is data |
