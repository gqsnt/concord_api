Open Library work records serve `description` (and some author `bio` fields) either as a plain
string or as a text object `{"type": "/type/text", "value": "..."}` (Open Library data dumps /
works API; cite the page). Make the work lookup (and the author lookup, where the slice reads
`bio`) accept both shapes and expose one plain text value to callers, keeping an explicit "absent"
distinct from an empty string. Add the `sort` option of the search API (e.g. `new`, `old`,
`random`, `key`) to the works search, with the values the documentation lists.
