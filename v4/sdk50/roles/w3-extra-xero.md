## Slice-specific notes

Requests carry the tenant chosen by the caller (`xero-tenant-id`); a token or an invoice must
never move across tenants. Refresh requires `offline_access`, and each refresh returns a new
refresh token that must be saved atomically with the access token — a crash or a concurrent
refresh must not leave the old one as the only copy. `concord-auth` has a scoped token cache with
single-flight refresh — read its README, evaluate it against this rule, and record exactly what it
could not express. Responses are XML unless JSON is requested; choose and justify. Handle 429.
