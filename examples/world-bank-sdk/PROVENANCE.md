# Provenance

Checked 2026-11-06: World Bank Indicators API **v2**, [API Basic Call Structures](https://datahelpdesk.worldbank.org/knowledgebase/articles/898581-api-basic-call-structures), sections Query Strings, Delimiters and Responses; [Indicator API Queries](https://datahelpdesk.worldbank.org/knowledgebase/articles/898599-indicator-api-queries), sections Sample Request/Response Format. The pages did not expose a document revision or last-updated date; v2 plus the access date pins the documentation used, not a claimed immutable revision. The additional [V2 enhancements](https://datahelpdesk.worldbank.org/knowledgebase/articles/1886674-new-features-and-enhancements-in-the-v2-api), “Enhancements in JSON responses for indicator data queries,” says facts are JSON numbers. No live API calls or vendor response bytes were used.

All fixtures are hand-authored and synthetic. The two-element metadata/data shape and metadata field names follow the documented JSON indicator example; the observation fields are a synthetic country-indicator adaptation, not copied from that indicator-description example. Dates, numbers, country and indicator labels, last-update date, pagination and whitespace are invented for tests and make no factual claim about World Bank data.

| Fixture | Source/section and transformations |
| --- | --- |
| `year.json` | Basic Call Structures, Date and Date-Range and Responses; Indicator API Queries, Sample Response Format. Adapted the array shape to a country observation with synthetic numeric zero and one page. |
| `page-1.json`, `page-2.json` | Same sources plus Basic Call Structures, Page and Per_page. Invented two one-record pages; page 1 contains null and page 2 zero, with both string and numeric `per_page` representations. |
| `empty.json` | Same array shape, synthetic null data and empty paging metadata; an edge-case assumption, not an example claimed by these pages. |
| `error.json` | Basic Call Structures, Sample invalid request response, XML error id 120, key and text; hand-translated to a synthetic JSON error array. The cited documentation does not show this JSON error shape. |
| `malformed.json` | Synthetic incomplete observation for a negative decode test, based on the same array structure. |

Other invalid responses written inline in the tests are also synthetic. Each fixture's bytes are protected from checkout newline conversion by `.gitattributes`.
