## Slice-specific notes

HubSpot uses OAuth access tokens refreshed with a refresh token, and a refresh may return a
replacement refresh token. Concurrent requests must not refresh twice or overwrite a newer
replacement with an older one: `concord-auth` has a scoped token cache with single-flight refresh
— read its README, evaluate it against this rule, and record exactly what it could not express. A
timed-out contact create is uncertain: surface it, never resend automatically.
