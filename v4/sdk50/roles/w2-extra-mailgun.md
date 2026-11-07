## Slice-specific notes

Mailgun authenticates with HTTP Basic (`api` / key): `concord-auth` provides Basic credentials —
read its README and use it if it fits. The send endpoint takes `multipart/form-data`
(`concord-codecs` feature `multipart`). An attachment may come from a non-rewindable source: the
SDK must never replay a send whose body cannot be rebuilt, and a timeout after a send is an
ambiguous write. Regional endpoints (US/EU) select the base URL.
