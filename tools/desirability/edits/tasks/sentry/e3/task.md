# sentry / e3: optional query value on every list operation

**Provider changelog.** Both list operations of this slice, the organization issue list
(`GET /api/0/organizations/{org}/issues/`) and an issue's event list
(`GET /api/0/organizations/{org}/issues/{id}/events/`), accept an optional `environment` query
value restricting results to one environment (for example `production`). Unset, nothing is sent
and every environment is included, as before. The pagination `Link` keeps the filter. Nothing
else changed.

Public names the hidden check uses:
- `organization(org).issues().set_environment(env)` and
  `organization(org).issue(id).events().set_environment(env)` send `environment=<env>`
  (`env`: `&str` or `String`).
