# Provenance: JSON Lines seeds

Synthetic codec seeds copied byte for byte from the 4.4 extensions handoff
(`fixtures/jsonl/`, described in its `fixtures/README.md` and `fixtures/cases.json`, schema
version 1). They were authored for the handoff; none is a captured provider response, a live
interoperability result or personal/production data. `.gitattributes` keeps their exact bytes
(CRLF, byte order mark, missing final LF) on every platform.

| File | Case | SHA-256 | Documented expectation | Asserted by |
|---|---|---|---|---|
| `values.jsonl` | J01 | `c6a3aa3f7cfca03932abc549150fe8b6077e3cd0164215de1b8126dcc88a2471` | records `{"name":"Zoë"}`, `null`, `[1,2]`, `"a\nb"`, `12`, `true` (first line ends with CRLF) | `jsonl_framing::seeds_match_documented_expectations` |
| `no_final_lf.jsonl` | J02 | `c4b5e8c2cfb9818791b2df904a604c6d08a2ae8b064cad5475957e757b63f704` | records `{"id":1}`, `{"id":2}` (no LF after the last) | same |
| `blank_line.jsonl` | J03 | `c882b8e961db20fe7a66d2dee075bc682a8cbb70f36e1f4595432adfaf23adc9` | strict: `blank_record` error after prefix `{"id":1}` | same |
| `blank_line_lenient.ndjson` | J04 | `c882b8e961db20fe7a66d2dee075bc682a8cbb70f36e1f4595432adfaf23adc9` | `ignore_blank`: records `{"id":1}`, `{"id":2}` | same |
| `truncated.jsonl` | J05 | `a7b29d38dab534bebe5c7437c9473044bbcc0ef2d2e0ce468f375e63368cf409` | `invalid_json_record` error after prefix `{"id":1}` | same |
| `bom.jsonl` | J06 | `8850347c2ffd603f34414bddc27933f284bd13ad4d762a9b65ce863f6f887161` | `bom_rejected` error, no records | same |
| `empty.jsonl` | J07 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` | no records | same |

The test embeds each file (`include_bytes!`) and compares it with the exact expected bytes
written inline (so a line-ending conversion is caught), then runs the finite `Decode` and the
incremental reader under every single split and representative multi-split partitions. The
handoff's error categories map to `ReadErrorKind::Format` with these labels: `blank_record` "a
line is blank, which this profile refuses", `invalid_json_record` "a line ends inside a JSON
value", `bom_rejected` "the input starts with a byte order mark, which this profile refuses".
