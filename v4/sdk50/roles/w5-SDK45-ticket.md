Callers need precise searches: the arXiv API's `search_query` combines field prefixes (`ti`,
`au`, `abs`, `cat`, `all`, …) with the Boolean operators `AND`, `OR` and `ANDNOT` and grouping,
and results can be sorted with `sortBy` (`relevance`, `lastUpdatedDate`, `submittedDate`) and
`sortOrder` (`ascending`, `descending`). Read the user manual's query construction and sort
sections and cite them with the date:
- https://info.arxiv.org/help/api/user-manual.html

Add a typed query builder that a caller cannot make syntactically invalid (fields and operators
from the documented sets; terms escaped or quoted as the manual describes) and the sort options;
paging, pacing and the 30,000-result window behave as they do for term search. Keep term search
and fetch-by-ids unchanged for existing callers.
