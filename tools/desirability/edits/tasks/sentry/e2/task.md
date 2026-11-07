# sentry / e2: shared header of a scope

**Provider changelog.** Sentry now requires every organization-scoped request (everything under
`/api/0/organizations/{organization}/`, issues, events and status updates included) to repeat
the organization id or slug in the header `X-Sentry-Organization: <organization>`, exactly as
given in the path (unencoded). Nothing else changed.

Public names the hidden check uses:
- every operation reached through `sentry.organization(org)` sends
  `X-Sentry-Organization: <org>`.
