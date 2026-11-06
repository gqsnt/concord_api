# Consumer fixture provenance

Hand-authored on 2026-11-06 for `tests/first_use.rs`; no provider calls or
captures, and no reuse of the SDK author's fixtures.

The [basic-call documentation](https://datahelpdesk.worldbank.org/knowledgebase/articles/898581-api-basic-call-structures)
supplies the country/indicator path and date, range, JSON and paging queries.
The [indicator-query documentation](https://datahelpdesk.worldbank.org/knowledgebase/articles/898599-indicator-api-queries)
illustrates a metadata/data JSON array for the catalog, including a string
`per_page`. The SDK README and public rustdoc describe country observations,
nullable values, page metadata and the one-element provider error envelope.
These consumer fixtures combine those documented contracts; they are not
verbatim official observation examples.

- `year.json`: a synthetic 2006 observation, numeric value 123456.
- `range-1.json`: two synthetic observations, one numeric and one null.
- `range-2.json`: a final synthetic observation with a real numeric zero;
  numeric `per_page` contrasts with the string on page one.
- `provider-error.json`: a hand-authored error envelope with a synthetic
  explanation; the chosen identifier/key are test data, not a claim about
  which error a live server would return.
- `truncated.json`: deliberately incomplete JSON bytes with no final newline;
  used with an interrupted HTTP chunked response. The tests also interrupt
  HTTP after sending a complete JSON document, to distinguish transport EOF
  from JSON completeness.

Tests construct empty pages and malformed variants by editing these synthetic
JSON objects with `serde_json`. Fixture files are covered by the SDK's existing
`fixtures/** -text` rule. No numeric value is an economic/population assertion.
