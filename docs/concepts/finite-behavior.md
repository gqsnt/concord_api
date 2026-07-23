# Finite behavior

Concord makes cumulative work explicit and bounded:

- request body preparation and multipart serialization;
- buffered and streaming successful responses;
- captured terminal status bodies;
- provider request and response bodies;
- record-decoder staging;
- pagination pages, accumulated items, and retained identities;
- endpoint and rate-limit identities;
- limiter state and server-directed cooldowns;
- authentication recovery and provider reacquisition.

The runtime withholds the first excess body, stream chunk, record staging
append, page, item, or identity rather than accepting an over-limit unit and
failing afterward. Pagination is sequential and cycle-detecting, returns all
items only on success, and never exposes a partial result under its existing
terminal.

Sealed response acquisition modes prevent an extension from introducing an
unbounded/native response escape. Multipart remains inside the private
transport boundary so aggregate size and replayability are known before an
authentication recovery attempt.

Custom extensions must bound their own internal CPU, memory, retries, maps,
queues, storage, and synchronous work. Concord bounds semantic inputs and its
own lifecycle but cannot infer arbitrary downstream implementation state.

Exact normative defaults and the setters that replace them are listed in
[Limits](../reference/limits.md). Unsupported unbounded or partially managed
forms are explicit in [Unsupported forms](../reference/unsupported.md).
