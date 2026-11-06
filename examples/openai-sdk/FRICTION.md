# Concord observations

Ordered by implementation sequence; line numbers refer to `src/lib.rs` after formatting.

1. **Concord helped — declaration.** The API declaration at `src/lib.rs:20` expresses methods, paths, sensitive authorization, and JSON/Binary formats together; the generated path parameter handling and base override allowed loopback testing without private access. Cause: Concord.
2. **Concord/docs — provider errors.** `src/lib.rs:291` uses `receive`, copies the response head, reads the unsuccessful body with `bytes_limited`, and separately decodes OpenAI's envelope. The generic declared `send` does not deliver both the typed provider error and response metadata as a single result. Reproduce with the 400 response in `tests/protocol.rs`; metadata is preserved by this adapter.
3. **Concord — multipart.** `src/lib.rs:378` constructs the entire multipart body and boundary and declares a binary request with a matching content type. Concord has no built-in multipart encoder for this form. `examples/multipart/src/lib.rs` supplied the public-API options; this SDK uses an explicit fixed boundary with collision refusal. Reproduce with `multipart_exact_bytes_and_validation`.
4. **Concord — SSE.** `src/lib.rs:342` obtains the native response and `src/lib.rs:500` implements line framing, event decoding, completion and limits. No Concord SSE decoder was found. The stream failure test reproduces the need to preserve events already delivered and distinguish EOF from completion. The loopback perturbation splits both a delimiter and UTF-8 bytes.
5. **SDK/application — batch JSONL and polling.** `src/lib.rs:171`, `src/lib.rs:187`, and `src/lib.rs:430` validate IDs, retain individual parse failures and implement bounded polling; these are provider-specific rules. Concord's JSONL support exists, but this implementation parses buffered file bytes per line to retain malformed rows and combine two files without losing successes. The caller chooses polling bounds and cancellation.
6. **Concord/docs — base URL override.** Initially declaring `/v1` in `base` caused `with_base_url(server.url())` to replace the version prefix as well; tests saw `/responses` instead of `/v1/responses`. The separate `path "v1"` at `src/lib.rs:24` preserves it. This behavior is reproducible with the wire tests and is a documentation/discoverability issue rather than a library change.

## Between waves 2-3 (coordinator integration)

Items 2 and 4 changed. The client declares `send SendPolicy;` (the former `send` helper),
so every generated `send()` returns the typed provider error and metadata. `response_stream` is
declared `-> stream`; the SDK parser reads the `Incoming` body through `into_response()`.
Multipart (item 3) and the parser stay SDK-local. See [INTEGRATION.md](INTEGRATION.md).
