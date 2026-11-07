## Slice-specific notes

Zendesk uses OAuth bearer tokens (`concord-auth` has bearer credentials). A 429 can carry
`Retry-After`: honour it for safe reads only (`concord-retry` exists for bounded retries of safe
reads; `concord-ratelimit` for admission/cooldown — read both READMEs and use them only if they
fit, recording why not otherwise). A timed-out ticket create is uncertain: surface it without
creating a duplicate.
