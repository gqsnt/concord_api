# Finite behavior

Concord gives every core-owned cumulative collection or retained structure a
finite bound or a hard structural ceiling. It fails closed at that boundary.
This contract does not make arbitrary application code finite: codecs,
providers, controllers, limiters, observers, hooks, and stream consumers
remain trusted work with their own limits.

Exact current numbers live only in the [Limits reference](../reference/limits.md).

## Core-owned bounds

| Surface | Core behavior |
|---|---|
| Public request shaping | caps each formatted value, component count, logical URL, header count, and aggregate header bytes |
| Request bodies | bounds cumulative nonempty bytes and successful empty chunks across bytes, streams, factories, and multipart |
| Accepted responses | bounds buffered or streamed post-decompression bytes and accepted empty chunks |
| Terminal status bodies | retains only a complete body within its cap; otherwise records a structural unavailable/omitted state |
| Provider HTTP | separately bounds request and response bodies under the credential operation deadline |
| Credential material | bounds retained strings and final private header/query encodings |
| Record delivery | combines the applicable buffered/streamed response limit with a decoder-selected positive staging limit |
| Pagination | runs sequentially with call-site, page, item, retained-identity, and repeated-request bounds |
| Rate plans | bounds rule IDs, rule count, key parts, formatted aggregate keys, and windows |
| First-party local limiter | uses finite quota/cooldown capacities, cooldown caps, and idle pruning |
| Managed retry | combines a hard per-operation maximum with bounded origin-scoped rolling-budget state |
| Runtime endpoints | incrementally bounds structure and aggregate public shape before a request can execute |

The configured runtime setters can replace selected defaults. They cannot
raise hard `MAX_*` ceilings or create an unbounded sentinel. Positive count
and byte settings use `NonZeroU32` or `NonZeroUsize`.

## First-excess behavior

A limit is checked before an excess unit becomes a successful result:

- request and response collectors do not retain the first excess nonempty
  bytes;
- streaming response consumers do not receive a chunk that would cross the
  cumulative byte ceiling;
- record delivery keeps source bytes separate from staging and rejects the
  first byte that would make one logical record exceed the decoder's limit;
- public formatting stops retaining output at its hard capture ceiling and
  returns an error rather than a truncated value;
- pagination never returns an item beyond the aggregate item ceiling and does
  not start a page beyond its page ceiling;
- pagination identity encoding withholds the first excess encoded byte;
- generated and runtime rate keys reject the first excess component or byte;
- local limiter insertion preflights all missing state required by the current
  plan and inserts none if the complete set cannot fit.

Unknown-length terminal status capture is intentionally different from a
truncated preview: when collection crosses its cap, the retained prefix is
discarded and the body state becomes `OmittedLimitExceeded`. Known excessive
length can be rejected before polling.

Limits report stable categories and structural states. They do not include the
excess bytes, cursor, formatted value, quota key, or native error in their
diagnostics.

## Empty chunks and progress

An empty transport chunk carries no application bytes but can still consume
CPU indefinitely. Request and response paths therefore maintain a separate
total empty-chunk ceiling. The count does not reset after nonempty data.

Core also limits ready-empty work per poll or async turn, wakes or yields, and
lets the executor regain control. Existing deadlines can then fire. Empty
chunks:

- are not sent to native request transport or yielded to response consumers;
- do not consume nonempty byte limits;
- do not advance an exact request length;
- fail before later EOF length validation or response decoding when their own
  ceiling is crossed.

Record decoding has an additional progress rule. A decoder cannot grow the
staging buffer, and returning `Some(record)` requires a strict net decrease.
That prevents an implementation from emitting an infinite sequence from the
same staged bytes, although the decoder must still bound its own synchronous
CPU. Pending source bytes are separate from decoder staging. Core can feed a
large transport chunk incrementally, yielding many small records, without
requiring the whole source chunk to fit the staging limit.

## Pagination and quota coherence

Pagination is sequential and all-or-error. Each page crosses the same managed
request lifecycle, and the aggregate vector becomes visible only after
natural or explicitly requested completion. A later page failure does not
return the accumulated prefix.

Core derives a canonical request identity for every prepared page and rejects
an exact repeat. The retained identity bytes and identity count are bounded
separately. A custom controller still must keep its own state finite and
produce a next state that changes the shaped request.

Pagination-controlled values can change between pages. The compiler rejects
every effective rate partition that directly depends on such a client, scope,
or endpoint value. It also follows final host-expression dependencies and
rejects `by [host]` when pagination could move the host. One aggregate
operation therefore cannot silently migrate through generated local quota
buckets.

The first-party local limiter preserves active history under state pressure.
It prunes only eligible idle/full quota state or expired cooldowns. If the
complete missing set does not fit, it returns recoverable
`RateLimitErrorKind::StateCapacity`, inserts none of it, and leaves existing
buckets operational.

## Authentication and attempts

A visible Concord attempt may contain Reqwest's private known-safe protocol
recovery. That recovery is enabled by default, performs no status retry, and
does not create new limiter admissions, hook attempts, or completion events.
An explicit managed policy retry is instead a new visible attempt.

Authentication challenge recovery permits at most one additional visible
application attempt and only when exact-current credential material is
attributed and the request body is replayable or reconstructible. Provider
acquisition and refresh are coalesced per credential slot, bounded by the
operation deadline, and use separately bounded provider HTTP.

These runtime limits do not excuse an application-owned retry loop. Its total
attempts, delay, queueing, and idempotency remain application policy.

## Trusted extension work

Concord bounds the inputs and outputs it owns. It cannot infer or preempt all
work inside application callbacks:

| Extension | Concord supplies | Extension must bound |
|---|---|---|
| buffered codec | bounded bytes and finite context | CPU, expansion, decoded heap, caches |
| request/response specification | sealed managed body modes and configured byte caps | synchronous preparation/conversion and owned state |
| record decoder | bounded source bytes and staging buffer in buffered or streaming delivery | parsing CPU, decoded values, side tables |
| credential provider | finite slot plus bounded `ProviderHttp` | caches, internal retries, tasks, external effects |
| pagination controller | one decoded page and operation-local state | callback time and controller allocation |
| limiter/observer | bounded immutable plan and sanitized head | quota storage, backend calls, overload policy |
| retry classifier | finite failure context | synchronous policy work, shared state, labels |
| runtime hooks | sanitized finite event structs | queues, exporters, labels, completion timeout |
| application stream consumer | cumulative wire/staging bounds | consumption deadline, concurrency, retained items |

Synchronous callbacks cannot be interrupted; a deadline is checked only after
they return. Asynchronous extension futures can be dropped on cancellation,
but effects already performed are not rolled back. Completion hooks run
outside the operation deadline and can delay delivery forever unless the
application bounds them.

The central liveness details are in
[Hooks, trusted work, and bounds](../extending/hooks-and-trusted-work.md).

## Related

- [Limits reference](../reference/limits.md)
- [Errors](../reference/errors.md)
- [Security and redaction](security-and-redaction.md)
- [Managed lifecycle](managed-lifecycle.md)
- [Unsupported forms](../reference/unsupported.md)
