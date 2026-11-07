## Slice-specific notes

Requests use a bearer token. List responses carry `links.pages` in the body; the next link may be
absent when there is no next page — absence means completion. A followed link must stay on the
API host: the token is never sent to a host the response named.
