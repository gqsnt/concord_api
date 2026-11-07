## Slice-specific notes

Intercom needs a bearer token and an `Intercom-Version` header. A 429 may arrive without
rate-limit headers: the SDK must not busy-loop or invent a reset time. `concord-ratelimit` and
`concord-retry` exist — read their READMEs, use them if they fit, and record in `FRICTION.md` what
they could not express.
