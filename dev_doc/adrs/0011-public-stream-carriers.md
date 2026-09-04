# ADR 0011 — Separate `ByteStream` and `ItemStream<T>`

- **Status:** Selected and supported by comparative lifecycle, allocation, throughput, compile, and size evidence

## Decision

Expose representation-independent `ByteStream` for non-semantic byte chunks and `ItemStream<T>` for semantic decoded items. Keep decoder/acquisition driver types private.

## Property earned

Caller-visible lifecycle, errors, limits, and item boundary stability are explicit without wrapper towers or driver compatibility commitments.

## Contract

Initial request completes on stream acquisition. Later polling has independent failures/limits/events. No transparent retry after yielding. Drop cancels local consumption only. No `Clone` without multicast/replay semantics.

## Release evidence

The selected implementation uses core-sealed generic acquisition drivers and
private boxed stream erasure for `ByteStream` and `ItemStream<T>`. The maintained
comparison covers monomorphized, private-enum, and boxed alternatives. Boxed
erasure uses one allocation at construction and none per poll, stays inside the
precommitted byte and item throughput budgets, has identical probe binary size,
and preserves the same lifecycle signature. Runtime tests separately cover
post-return failure, bounds, backpressure, cancellation, and terminal evidence.
See `dev_doc/evidence/stream_driver_comparison.md`.

The public carriers are frozen for v2.1. A future implementation may change
only the private erasure strategy if a maintained measurement crosses a budget.
