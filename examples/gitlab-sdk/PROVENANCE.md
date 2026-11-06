# Provenance

## Sources checked

All pages were read on **2026-11-06** with a web fetch tool (rendered page text, not the
documentation repository). None of them shows a page date. The documentation is the current
docs.gitlab.com, covering GitLab 19.x: the Projects API's "Retrieve a project" history lists
changes up to GitLab 19.4. No GitLab instance, account or token was used; no request was made to
any GitLab API.

| Page | What was taken from it |
|---|---|
| [REST API](https://docs.gitlab.com/api/rest/) | base path `/api/v4` (major version 4); namespaced paths must be URL-encoded (`diaspora%2Fdiaspora`); offset pagination is the default and available on all endpoints except `users`; `page` (default 1), `per_page` (default 20, max 100); `Link` with `prev`/`next`/`first`/`last` and "use these links instead of generating your own URLs"; the headers `x-next-page`, `x-page`, `x-per-page`, `x-prev-page`, `x-total`, `x-total-pages`, `x-request-id`; `x-total`, `x-total-pages` and `rel="last"` are not returned over 10,000 records; keyset pagination per resource, with project issues listed from GitLab 18.3; the API redirects only `GET` requests after a project path change; the `PRIVATE-TOKEN` header in examples |
| [Issues API](https://docs.gitlab.com/api/issues/) | "List project issues" (`GET /projects/:id/issues`: `id` is the ID or URL-encoded path; the attributes `state`, `labels` (comma-separated, `None`/`Any`), `milestone`, `search`, `author_username`, `order_by` (offset values, default `created_at`), `sort` (default `desc`), `cursor` (keyset); "supports both offset-based and keyset-based pagination"; keyset recommended for consecutive pages; 20 results by default) and "Retrieve a project issue" (`GET /projects/:id/issues/:issue_iid`, `subscribed` in the response); the issue object's keys, including `_links`, `references`, `time_stats`, `task_completion_status`; the `403` body `{"message": "403 Forbidden"}` |
| [Projects API](https://docs.gitlab.com/api/projects/) | "Retrieve a project" (`GET /projects/:id`); the project object's keys, `namespace` and `_links` keys |
| [REST authentication](https://docs.gitlab.com/api/rest/authentication/) | access tokens in `PRIVATE-TOKEN` (recommended) or `Authorization: Bearer`; invalid or missing authentication is `401` with `{"message": "401 Unauthorized"}` |
| [REST troubleshooting](https://docs.gitlab.com/api/rest/troubleshooting/) | the status-code table (incl. `429`: wait for `Retry-After`); the error message forms `{"message":"400 (Bad request) \"title\" not given"}` and the validation object `{"message": {"bio": [...]}}` |

What the pages did **not** document, and the SDK therefore does not assume: a `404` body for a
missing issue or project; a `429` body; any `{"error": ...}` form; whether GitLab's `Link` keeps
the requested path form (encoded path or ID) and query order. The SDK accepts either path form
in a link and treats the query as opaque.

## Fixtures

Every fixture is **hand-authored**. Values (IDs, names, users, dates, hosts) are synthetic;
`gitlab.example.com` is the documentation's example host. Field names and shapes follow the pages
above. No fixture was downloaded or copied from a response. Bytes are pinned by
`.gitattributes` (`fixtures/** -text`).

| Fixture | Source | Transformations / synthetic parts |
|---|---|---|
| `issues-page-1.json` | Issues API, "List project issues" response | two issues (`iid` 3 and 2) with the documented keys; synthetic values; `labels`, an assignee, a milestone on one; a non-ASCII title |
| `issues-page-2.json` | same | one closed issue (`iid` 1) with `closed_by` |
| `issues-empty.json` | REST API, pagination (an empty array) | `[]`; synthetic |
| `issue.json` | Issues API, "Retrieve a project issue" response | issue `iid` 2 with `subscribed`; keys reordered, synthetic values |
| `project.json` | Projects API, "Retrieve a project" response | a subset of the documented keys (`id`, names, paths, `default_branch`, `visibility`, `issues_enabled`, `open_issues_count`, `namespace`, `_links`, ...); synthetic values; the many CI/settings keys omitted |
| `project-mismatch.json` | same shape | **synthetic negative case**: another project (`id` 4022) |
| `unauthorized.json` | REST authentication, invalid or missing authentication | the documented body as shown |
| `forbidden.json` | Issues API, `403` example | the documented body as shown |
| `validation.json` | REST troubleshooting, validation error | the documented example body (a `bio` field: not an issue attribute); used only to decode the object form of `message` |
| `issue-not-found.json` | — | **synthetic**: `{"message":"404 Issue Not Found"}`, modelled on the documented `"<status> <reason>"` message texts; not documented |
| `project-not-found.json` | — | **synthetic**, as above, for a project |
| `rate-limited.txt` | — | **synthetic** plain-text `429` body; the documentation names `Retry-After` but no body |
| `proxy-error.html` | — | **synthetic** HTML page of a proxy in front of GitLab |
| `malformed.json` | — | **synthetic** truncated JSON (no trailing newline, by design) |

## Values written in the tests

- **Response headers** (`tests/support/mod.rs` `offset_headers`, `link`): the documented header
  names and `Link` form `<url>; rel="..."`; numbers, request ids and URLs are synthetic. Link
  targets use the loopback server's address. The two-page script's links put `page` and
  `per_page` first, like the documented example, so they differ in order from the request.
- **Perturbation** (`a_follows_the_returned_link_whatever_its_representation`): a `next` link
  addressing the project by ID with `cursor=synthetic-opaque-cursor`, another `per_page`, extra
  and reordered parameters, and no `x-next-page` — **synthetic**, to show that the returned link
  is requested as sent. It is not a documented GitLab cursor format.
- **Maintenance perturbation** (`changed_link_query_survives_cap_and_failed_retry`): the initial
  size 3, linked size 1, opaque encoded cursor, repeated `tag`, empty value, `+` query spelling,
  and conflicting `x-next-page` value are synthetic. Existing hand-authored issue fixtures are
  reused; this is not a documented GitLab response or cursor format.
- `{"message":"500 Internal Server Error"}` (a failed page), `Retry-After: 30`, the `301`/`302`
  redirects, the oversized `500` body, the malformed and foreign `Link` values and the HTML `200`
  are **test inventions** for negative cases.
- The token `synthetic-fixture-token` is synthetic and deliberately not in GitLab's token format.
