## Slice-specific notes

Requests use a bearer token. Pagination uses the `Link` header, whose entries carry a cursor and a
`results` flag: a next link with `results="false"` ends the listing. Sentry documents separate
frequency and concurrency limits; a retry schedule must not ignore the concurrency limit.
`concord-pagination`, `concord-retry` and `concord-ratelimit` exist — read their READMEs, decide,
and record exactly what they could not express.
