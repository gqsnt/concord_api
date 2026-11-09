Net glue removed (hand tally, GLUE-COUNT.md): distribution-registry 45; amazon-s3 2; nager-holidays 0 nonblank lines; source net after declarations: 43/1/0.
New concepts: `error D in F`, `Error<D>`, `Failure<D>`, five-state `Document<D>`; typed carriers, provider extraction, declared-codec `failure(limit)` and Rust error-body bound.
C1: The declaration states the provider's error document and explicit codec; read bounds stay in Rust.
C2: Body reading/storage moves to core; challenge replay, signing, 304/416 decisions and recovery stay in Rust.
C3: Nager declaration, source and unread-error behavior are identical; its caller uses the existing API.
C4: Explicit error codec, nearest-owner inheritance, documented 64 KiB default; success codecs and request bytes stay independent.
C5: Codec/document and policy/document mismatches must diagnose the authored statement; required compile-fail checks are in TESTS.md.
C6: “Failed statuses carry the declared document”; policy generics and the compatibility facades remain a visible cost.
C7: Native client, prepare/receive, raw send and into_parts remain; consumed failures keep head and bounded bytes instead of an unread response.
C8: One bounded read, no new actor/retry/global state; existing SDK challenge behavior stays explicit.
C9: Complete changed tests and a new five-state native S3 specification are supplied; all remain unexecuted pending A1 support.
Simple control changed: No (two byte-identical source copies; one added caller example).
Does the declaration read better than before? Yes: one explicit line exposes failure shape across mixed success codecs; S3's 2-line net glue saving is marginal.
