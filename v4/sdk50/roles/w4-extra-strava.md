## Slice-specific notes

Tokens are refreshed through OAuth. Strava enforces two usage windows (a short one and a daily
one) and reports both in response headers; a 429 can come from either. `concord-ratelimit` and
`concord-retry` exist — read their READMEs, decide, and record exactly what they could not
express. An activity upload is a multipart request with a file that may be read only once: a
timed-out upload is ambiguous and is never replayed automatically; the caller polls the returned
upload id until processing finishes or fails.
