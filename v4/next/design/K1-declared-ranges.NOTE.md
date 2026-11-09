Glue removed by hand (nonblank removed items, including docs): github 31 (28 wrapper + 3 error lines); mailgun 22; nager-holidays 0; see DESIGN.md for the classifier distinction.
New concept: `in <constant inclusive range>`; existing params, Rust NonZeroU16 and inline const are reused.
C1: GitHub's range and Mailgun's cap are facts; Mailgun's lower endpoint is its Rust type's MIN, not a claimed provider minimum.
C2: scalar bound glue moves to declarations; nonzero policy stays in Rust's type; relational/length/stream checks stay in Rust.
C3: Nager's declaration is byte-for-byte unchanged; no invented year bound.
C4: explicit typed bounds; existing parameter groups preserve placement and query order; each validates items.
C5: Build names value, bound and endpoint; compile diagnostics must cite authored bounds; failure is later than construction.
C6: GitHub reads compactly; Mailgun's typed constants are a readability cost and a limit of the general design.
C7: caller transport, prepare, send_raw and into_parts remain available.
C8: final-value comparisons before I/O; no retries, clocks, state or extra requests introduced.
C9: supplied boundary/wire/no-I/O/each/canary tests are unrun; compiler diagnostic cases remain acceptance obligations.
Simple control changed: no SDK changes; complete unchanged lib.rs and one caller are supplied.
Does the declaration read better than before? No overall: GitHub does, but Mailgun's NonZeroU16 constant range is harder to read than PageSize.
Complete replacements mirror repository paths under each SDK directory; TESTS.md lists migrations; no Concord implementation or compilation.
