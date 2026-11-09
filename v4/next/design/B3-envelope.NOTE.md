Glue removed (hand-audited code lines; GLUE.md): Slack 110; Kraken 23; Frankfurter 0. Direct B3 alone: +1 / −2 / 0; most savings require B1/A1.
New concepts: envelope E<_>, Envelope::{Value,Rest,Error}/open, Refusal<D>, Received::open, Response::rest, Refused; prerequisites Response<T,R>, error D in F, Error<D>/Failure<D>.
C1: envelope and error codec describe provider bodies; read bounds remain Rust choices.
C2: facts declared once; payload validation, refusal classification, signing and traversal stay Rust.
C3: Frankfurter src, declaration, manifest and README are byte-identical; only a caller example is added.
C4: arrow and send-template _ mean value; envelope _ makes the wire type; nearest owner applies.
C5: require value/error/codec mismatches at the authored statement; diagnostic cases are in TESTS.md.
C6: “A successful body is Envelope<value>; Rust opens it.” Slack's wire/value adapter is extra Rust cost.
C7: caller transport, prepare/send_raw/into_parts and raw-envelope sends remain; Kraken prepare stays unsigned.
C8: one dispatch/read, no automatic retry, refresh, sleep or new global state.
C9: existing fixture/canary obligations and new wire/compile-fail cases are listed in TESTS.md; none were run.
Simple control changed? No SDK behavior or surface change; source snapshots are unchanged.
Does the declaration read better than before? Yes: envelope is stated once and arrows name caller values; savings alone are weak.
Validation: formatted and statically reviewed only; no compilation, Concord changes, or provider traffic.
