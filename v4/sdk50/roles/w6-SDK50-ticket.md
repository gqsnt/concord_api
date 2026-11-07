Callers need the countries of a region and of a language: the Countries API documents reading by
the `region` / `subregion` property and by `languages`, alongside the existing name and code
lookups, with the same field selection and paging. Read the Countries API page and cite it with
the date:
- https://restcountries.com/docs/countries

Add typed region, subregion and language lookups (values validated before any request; the same
`response_fields` selection, page options and bounded walk as the existing listing; no-match is
an empty page, not an error). Keep name search, code lookup and the filtered listing unchanged
for existing callers.
