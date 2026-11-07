## Slice-specific notes

Requests use a bearer API token. Responses are envelopes (`success`, `errors`, `messages`,
`result`, `result_info`); an HTTP success whose envelope says `success: false` is a provider
error, never an empty result. Pagination metadata comes in the envelope and may differ per
endpoint. Global and per-service limits can both apply; read the rate-limit headers and handle
429 without assuming a single global token bucket.
