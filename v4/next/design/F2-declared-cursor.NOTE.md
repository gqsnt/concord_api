Hand counts are net nonblank glue lines; see COUNTS.md for block-by-block accounting.
Stripe: 13 removed (4 projection + 9 trait/macro bridge); +7 DSL/+4 explanation lines leave only 2 fewer source lines.
Mailgun: 0 removed; full URL continuation and eager search/credential validation stay in Rust.
PokeAPI: 0 removed; simple control source and declaration are byte-for-byte unchanged.
New concepts used: #[cursor], Cursor::at, page items/next/more, Rust paths/[last], Paged::{next,split}, PageError, is_paged.
C1: New declaration words state Stripe's forward relation; no generated-surface controls.
C2: Forward placement/projection are facts; traversal stays Rust; backward and URL fact glue remains.
C3: No concepts or lines imposed on PokeAPI; F5 numbered kinds are unused here.
C4: Explicit role and field paths; no inference from cursor names, offsets, counts or secrets.
C5: Compile-fail fixtures specify errors at duplicate/required attributes and the bad field path.
C6: Closed field-path grammar; the extra F5 vocabulary has no supporting witness in these three SDKs.
C7: Caller transport, custom formats, prepare, into_parts and send_raw remain available.
C8: No implicit I/O, retries, global binding state or pagination actors added.
C9: Existing tests keep their assertions; new acceptance/diagnostic fixtures supplied, not run (no compilation authorized).
Does the declaration read better than before? Yes: the forward wire relation is visible beside its types, though actual source savings are marginal.
