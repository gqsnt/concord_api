Callers need to post an event: Datadog's Events API accepts a new event (title, text, tags,
alert type, aggregation key, source) on the caller's site and answers that it was accepted. Read
the events API reference for posting an event and cite it with the date:
- https://docs.datadoghq.com/api/latest/events/

Add the post so a caller gets the provider's acceptance (never a claim that the event is already
searchable); inputs outside the documented limits are refused before any request; it uses the
same site host and both keys as the reads, with the same redaction and wrong-region rules; a post
that times out is uncertain (the event may exist) and is never resent automatically. Keep the
listing and search unchanged for existing callers.
