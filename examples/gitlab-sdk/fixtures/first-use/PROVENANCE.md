# Consumer-authored fixture provenance

These files were independently hand-authored on 2026-11-06 from the official
documentation below, read in the browser. They are not captured responses,
verbatim documented examples, or copies of the SDK author's fixtures.

- `issue-31.json`, `issue-32.json`: field names and JSON shapes from
  [list project issues](https://docs.gitlab.com/api/issues/#list-all-project-issues)
  and [retrieve a project issue](https://docs.gitlab.com/api/issues/#retrieve-a-project-issue).
  All identifiers, names, text, dates, labels, URLs, and the selected null/absent
  fields are synthetic. IDs deliberately differ from project-scoped IIDs.
- `project-73.json`: field shapes from
  [retrieve a project](https://docs.gitlab.com/api/projects/#retrieve-a-project).
  All values are synthetic. A null default branch models an empty repository.
- `not-found.json`, `forbidden.json`, `rate-limited.json`: the documented
  `message` envelope from [REST troubleshooting](https://docs.gitlab.com/api/rest/troubleshooting/)
  and the [SDK README](../../README.md); the response combinations and messages
  here are chosen test inputs, not evidence of an instance's exact responses.
- `empty.json`: a hand-authored empty JSON collection.
- `proxy-error.html`: invented proxy failure bytes, not a GitLab document.

The tests wrap the hand-authored issue objects in a JSON array for list responses.
They also deliberately remove a title, change a returned project ID, mislabel
content type, break Link headers, and cut off a chunked response. Those are
synthetic protocol faults. Link URLs/headers use the ephemeral loopback origin;
their parameter values/order/encoding are test inputs based on
[REST pagination](https://docs.gitlab.com/api/rest/#pagination). Empty intermediate
pages and inconsistent pagination headers are fault/edge scenarios, not claims
that GitLab normally produces them. Authentication uses a visibly synthetic
token through the documented PRIVATE-TOKEN header. No fixture URL is fetched.

The SDK root's [`.gitattributes`](../../.gitattributes) uses `fixtures/** -text`;
the rule covers all these bytes.
