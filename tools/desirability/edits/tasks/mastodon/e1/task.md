# mastodon / e1: version path prefix of one endpoint family

**Provider changelog.** Mastodon moved the timeline endpoints from `/api/v1/timelines/` to
`/api/v2/timelines/`: `GET /api/v1/timelines/public` is now `GET /api/v2/timelines/public` and
`GET /api/v1/timelines/tag/{hashtag}` is now `GET /api/v2/timelines/tag/{hashtag}`. Their query
values, documents and statuses are unchanged; every other endpoint (statuses, instance,
streaming) keeps its path. Nothing else changed.

Public names the hidden check uses (unchanged names, new behaviour):
- `Mastodon::public_timeline()` requests `/api/v2/timelines/public`;
- `Mastodon::hashtag_timeline(tag)` requests `/api/v2/timelines/tag/{tag}`;
- `Mastodon::status(id)` still requests `/api/v1/statuses/{id}`.
