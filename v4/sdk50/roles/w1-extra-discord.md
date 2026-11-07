## Slice-specific notes

Discord's rate limits are part of this slice (see the rate-limits page): route buckets identified
by `X-RateLimit-Bucket` together with the route's major parameter (channel/guild id), separate
global limits, 429 bodies with `retry_after` (seconds, possibly fractional) and `global`, and
`X-RateLimit-Scope`. Concord has no rate-limiting crate yet: implement what the slice needs in the
SDK, as simply as you can, and record in `FRICTION.md` exactly what you had to write (this is
design evidence for a shared rate-admission capability). A timeout after a message creation is an
ambiguous write: never resend automatically. `concord-retry` exists if you want bounded retries
of safe reads.
