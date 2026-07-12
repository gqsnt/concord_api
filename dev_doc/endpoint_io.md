# Endpoint I/O Design Contract

Endpoint I/O separates parser syntax, semantic resolution, generated adapter metadata, and syntax-neutral core execution.

The retained reserved families are `Stream<M>`, response-only `Bytes`, response-only `NoContent`, and request-only `Multipart<T>`. All other endpoint I/O uses the ordinary buffered codec path.

`Multipart<T>` is the sole supported request multipart shape. Generated request methods accept `MultipartBody`, which preserves Concord's temporary streaming form-data encoder, boundaries, filenames, MIME metadata, and producer errors. Requests are non-replayable.

`Stream<M>` remains the generic raw byte stream family. Its request value is `StreamBody`, its response value is `StreamResponse<M>`, and `.execute_stream()` is the dedicated raw-response terminal. Streaming request bodies are not replayable.

Request-entity preparation returns a single `PreparedBody`. It owns body production, media type, `http_body::SizeHint`, one-shot consumption, and replayability. Each physical attempt produces the `DynBody` owned by the standard HTTP request.

Sema owns the reserved-family classification and derives `RequestEntityPlanIr` and `ResponseEntityPlanIr`; code generation consumes those resolved entity plans and does not inspect raw syntax. Core owns `RequestEntity` and `ResponseEntity` execution but must not depend on macro AST or DSL spellings.

Pagination remains limited to buffered decoded responses. Runtime hooks, diagnostics, retry metadata, and rate-limit metadata remain body-free and auth-safe.
