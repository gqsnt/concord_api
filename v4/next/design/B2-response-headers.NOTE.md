Glue removed (hand ledger in DESIGN.md): Stripe 44 nonblank lines; Discord −21 (21 added); PokeAPI 0.
New concepts: meta Name(..) generates a typed header trait; #[request_id] marks a printable correlation field; each mirrors input placement; value grammars remain Rust FromStr.
C1: header names, types, multiplicity and correlation roles are provider facts.
C2: Stripe error classification and Discord admission/delay decisions remain Rust; exact Discord number grammars remain Rust.
C3: PokeAPI's complete source/declaration is byte-identical; it imports no new trait.
C4: explicit trait/field names, strict singleton reads and nearest-owner contract; no inferred correlation names.
C5: supplied UI fixtures require field-local diagnostics for bad names, missing parsers, duplicates and secret correlation fields.
C6: “These are the typed headers this provider returns”; no policy vocabulary.
C7: caller transports, prepare(), into_parts(), send_raw(), raw headers and body handling remain available.
C8: lazy parsing adds no I/O, retries, cache or global state; the existing explicit Discord limiter retains its state.
C9: supplied acceptance tests cover strict parsing, each, inheritance, parser-error scrubbing, correlation canaries and limiter fallbacks; none compiled/run here.
Simple control changed? No SDK change; only an added caller example.
Does the declaration read better than before? Yes: the response contract is visible once beside the request; Discord's exact parsing adapters cost more Rust, honestly.
Compatibility: Stripe metadata fields become strict methods (including absent replay=None); see DESIGN.md for malformed-header, PartialEq and diagnostic changes and the six migrated tests.
