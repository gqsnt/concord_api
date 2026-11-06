# Provenance

Sources rechecked 2026-11-06. Linear's pages do not display a version or publication date; the pinned reference is the unversioned page content accessed on that date, not a pinned server schema. No example response bytes were downloaded from Linear. All fixtures are hand-authored synthetic UTF-8 JSON with a final LF, with invented IDs, text, cursors, diagnostics, and extension values. They are examples of shapes, not vendor conformance evidence.

| Fixture | Official source and section | Transformations and synthetic cases |
| --- | --- | --- |
| `page-one.json` | [Pagination](https://linear.app/developers/pagination), forward cursors, `nodes`, `pageInfo`; [Getting started](https://linear.app/developers/graphql), queries | Combined the documented issue fields and connection shape into a first page, invented values and cursor. |
| `page-partial.json` | Same sections; [Getting started, Error handling](https://linear.app/developers/graphql#error-handling) | Invented a terminal page with `description: null`, a field error with path and extension, and a top-level extension. The exact error code is synthetic. |
| `issue.json` | [Getting started, Queries & Mutations](https://linear.app/developers/graphql) | Constructed a JSON response around the documented issue selection with invented values. |
| `issue-partial.json` | Same issue selection and Error handling | Invented nullable field failure, error location, path and extension. |
| `errors.json` | [Getting started, Error handling](https://linear.app/developers/graphql#error-handling) | Invented errors-only envelope and code, used for both HTTP 401 and HTTP 200 tests; no particular status/code pairing is claimed as documented. |
| `invalid-page.json` | [Pagination](https://linear.app/developers/pagination), `hasNextPage` and `endCursor`; Error handling | Deliberately inconsistent next-page metadata and invented error to test rejection and preservation. |

The SDK's fixed named documents and variable declarations adapt the documented inline queries to variables; exact request bytes are asserted in the tests. The synthetic body shapes and tests do not validate the currently deployed schema or actual server behavior.
