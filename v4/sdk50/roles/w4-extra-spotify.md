## Slice-specific notes

Access tokens come from the authorization-code flow and are refreshed with a refresh token; a
refresh response may omit the refresh token, and then the stored one stays valid — a missing
replacement must never erase it. Concurrent requests must not refresh twice. `concord-auth` has a
scoped token cache with single-flight refresh — read its README, evaluate it against this rule,
and record exactly what it could not express. A 429 carries `Retry-After`; a 403 can mean the
app is in development quota mode, not that the resource is forbidden.
