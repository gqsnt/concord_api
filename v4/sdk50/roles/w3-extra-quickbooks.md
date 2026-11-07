## Slice-specific notes

Every Accounting request is scoped to one company (realm) chosen by the caller; a request, a
token or a cached record must never cross realms. OAuth access tokens are refreshed with a refresh
token that the provider may replace: a replaced or expired refresh token must surface as a
recoverable authorization error. `concord-auth` has a scoped token cache with single-flight refresh
— read its README, evaluate it against this rule, and record exactly what it could not express.
Invoice creation is never replayed automatically; a timed-out create is uncertain.
