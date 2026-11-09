# concord-core

The runtime of [Concord](https://github.com/gqsnt/concord_api): request composition, native
headers, snapshots, the open `Encode`/`Decode` contracts and the error type. It depends on
reqwest, `bytes`, `url`, `percent-encoding` and `http-body` (already reqwest's), and on nothing
format-specific.

Applications normally depend on the `concord` crate, which re-exports this crate's public API
together with the `api!` macro and the standard formats. Depend on `concord-core` directly to
implement a format (`Encode`/`Decode`) without the macro or the standard formats.

The `__private` module is support code for `api!` expansions, not a public API.

Dropping `send()`, `receive()` or `Decoder::execute` before the response arrives cancels a
streamed request body. Cancellation and the checks before and after each body poll are ordered
under one lock: no new poll is admitted after cancellation; if cancellation precedes the
post-poll check, its result (including final data, trailers or end) is discarded in favour of an
error. The cancelled body reports no end and advertises no length. Results accepted before
cancellation cannot be recalled, even if delivered later: a request can still complete if the
transport has the whole body, including every byte of a declared length.

The drop synchronously waits for a poll in progress on another thread. A blocking source can
hold that thread indefinitely; a source waiting on the dropping thread can deadlock. A body
that drops the call inside its own poll avoids that wait: the rest of that poll can run after
the drop returns, but its result is discarded. Otherwise, once the drop returns, the body and
source are never polled again. Buffered bodies are not wrapped; after the response arrives,
any remaining upload belongs to the transport. See `Decoder::execute`'s cancellation rustdoc.
