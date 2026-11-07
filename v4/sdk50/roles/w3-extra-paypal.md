## Slice-specific notes

Use the sandbox base. Tokens come from client credentials (there is no refresh token): concurrent
requests must not fetch twice, and an expired token is fetched again. `concord-auth` has a scoped
token cache with single-flight refresh — read its README, evaluate it against this rule, and record
exactly what it could not express. `PayPal-Request-Id` makes only the endpoints that document it
safe to retry; a capture is never retried unless its request ID permits it. A timed-out capture is
uncertain: surface it.
