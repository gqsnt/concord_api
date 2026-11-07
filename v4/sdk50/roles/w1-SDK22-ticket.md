api.nasa.gov reports per-key quota on every response with `X-RateLimit-Limit` and
`X-RateLimit-Remaining` (api.nasa.gov "Web Service Rate Limits"; cite it). The application needs
the remaining quota after every successful call **and** after a failed one (including the quota
error itself), without ever exposing the API key. Expose it on the SDK's success and error results
for every endpoint of the slice, with a clear "absent/unparsable" outcome.
