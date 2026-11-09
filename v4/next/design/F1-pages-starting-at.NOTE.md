Glue removed by hand (nonblank SDK source, docs included): github 7 (3 code + 4 docs); discord-rest 4 (3 code + 1 doc); pokeapi 0; see DESIGN.md regions.
New concepts: Pages::starting_at, opt-in Pages::refuse_repeated_keys, PagesError::Repeated, Outcome::Repeated; no DSL concepts.
C1: All three declarations are byte-for-byte unchanged; traversal settings stay in Rust.
C2: The library owns starting/repeat behavior; GitHub link validation and Discord page validation stay in Rust.
C3: PokeAPI keeps its declaration, manual follow workflow and dependencies unchanged.
C4: Only explicit Resume and opt-in Eq/Hash key equality; no inferred URL or listing identity.
C5: New builders reject use after the first poll at the caller; Eq/Hash errors must name caller bounds.
C6: Two method names replace twins and repeated Resume prose; shared semantics live in the paging crate.
C7: Pages::new(from, factory), prepare(), native reqwest, send_raw() and into_parts() remain available.
C8: No construction I/O, hidden tasks or retries; repeat storage grows with admitted keys, capped only when max_pages is set.
C9: TESTS.md specifies equivalence, pending-poll refusal, repeated-key request counts, prefixes and canaries; no compile or tests run.
Simple control changed? No: PokeAPI src/lib.rs is an identical snapshot; browse.rs shows the existing caller workflow.
Does the declaration read better? No: it is identical; only the Rust traversal API and duplicated helper prose improve.
Compatibility: two public twins disappear; three tests migrate calls only; exhaustive PagesError matches elsewhere need Repeated.
Limit: GitHub's self-link error and Discord's unordered/oversized-page errors remain; replacing them would change accepted prefixes.
