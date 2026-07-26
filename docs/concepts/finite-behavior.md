# Finite behavior

Concord makes cumulative work explicit and bounded:

- request body preparation, streaming, and multipart serialization;
- buffered and streaming successful responses;
- empty transport-chunk progress across every body-acquiring response mode;
- captured terminal status bodies;
- provider request and response bodies;
- record-decoder staging;
- pagination pages, accumulated items, and complete encoded retained
  identity vectors;
- endpoint and rate-limit identities;
- limiter state and server-directed cooldowns;
- authentication recovery and provider reacquisition.

The runtime withholds the first excess body, stream chunk, record staging
append, page, item, or identity rather than accepting an over-limit unit and
failing afterward. Pagination is sequential and cycle-detecting, returns all
items only on success, and never exposes a partial result under its existing
terminal.

Generated rate-limit keys stop at the first excess part, name, or aggregate
byte and never retain unbounded formatted values. The first-party local
limiter similarly preflights every missing entry for a plan: capacity failure
inserts none, preserves active quota and unexpired cooldown history, returns
recoverable `StateCapacity`, and leaves existing buckets operational.

Pagination may update bound values between pages. The declaration compiler
therefore rejects every effective inherited or local rate partition that
depends directly on a pagination-controlled client, scope, or endpoint value.
It also follows final formatted host dependencies recursively and rejects
`by [host]` when pagination could change that host. A pagination sequence
cannot silently migrate between local quota buckets through generated state.

Empty chunks do not represent application body bytes and are never yielded or
forwarded. Core bounds their total count independently for one request body
and one response body; neither total resets after nonempty data. Request
chunks do not consume body bytes or advance an exact length. Response chunks
do not consume the applicable post-decompression byte limit. Core processes
only a bounded ready-empty run per poll or async turn, waking or yielding so
the source cannot monopolize the executor and existing deadlines can regain
control. A request empty-chunk excess wins before eventual EOF exact-length
validation; a response excess wins before later byte, status, or decode work.

Sealed response acquisition modes prevent an extension from introducing an
unbounded/native response escape. Multipart remains inside the private
transport boundary so aggregate size and replayability are known before an
authentication recovery attempt.

Custom extensions must bound their own internal CPU, memory, retries, maps,
queues, storage, and synchronous work. Concord bounds semantic inputs and its
own lifecycle but cannot infer arbitrary downstream implementation state.

Concord-owned acquisition and staging are byte-bounded. Application-owned
decoding, callback state, and callback work are trusted: a bounded wire body
does not bound the heap size of the decoded object, and an extension can
allocate or retain more state than its input bytes. The application must set
its own limits for those transformations and side effects.

Exact normative defaults and the setters that replace them are listed in
[Limits](../reference/limits.md). Unsupported unbounded or partially managed
forms are explicit in [Unsupported forms](../reference/unsupported.md).
