Callers need hashtag timelines. Mastodon documents `GET /api/v1/timelines/tag/:hashtag` (with
`any`/`all`/`none` extra tags, `local`, `remote`, `only_media`, `max_id`/`since_id`/`min_id`,
`limit`) and a hashtag stream on the streaming API (`stream=hashtag&tag=…`, and `hashtag:local`).
Read the timelines and streaming pages and cite them with the date:
- https://docs.joinmastodon.org/methods/timelines/
- https://docs.joinmastodon.org/methods/streaming/

Add the hashtag timeline (the hashtag is validated before any request and path-encoded as one
segment) and the hashtag stream (same instance and streaming-host binding, same framing and
terminal-disconnect rules as the public stream, no replay). Keep the public timeline, status
retrieval and the public stream unchanged for existing callers.
