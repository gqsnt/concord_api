# ADR 0011 — Separate `ByteStream` and `ItemStream<T>`

- **Status:** Selected for prototype; lifecycle supported, comparative erasure/performance gate provisional

## Decision

Expose representation-independent `ByteStream` for non-semantic byte chunks and `ItemStream<T>` for semantic decoded items. Keep decoder/acquisition driver types private.

## Property earned

Caller-visible lifecycle, errors, limits, and item boundary stability are explicit without wrapper towers or driver compatibility commitments.

## Contract

Initial request completes on stream acquisition. Later polling has independent failures/limits/events. No transparent retry after yielding. Drop cancels local consumption only. No `Clone` without multicast/replay semantics.

## Revisit

Benchmark enum, boxed, and monomorphized private drivers. Change only private erasure strategy unless evidence disproves the public distinction.

## Prototype evidence

The selected implementation uses core-sealed generic acquisition drivers and
private boxed stream erasure for `ByteStream` and `ItemStream<T>`. Lifecycle,
post-return failure, bounds, backpressure, cancellation, and terminal evidence
tests pass. The maintained runtime probe measures stream wrapping and record
framing, but it does not yet compare allocation/throughput/code-size across
boxed, enum, and monomorphized alternatives. Boxing is therefore a coherent
prototype choice, not a release-frozen performance conclusion.
