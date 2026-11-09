Glue removed, hand tally of net existing test code: Zendesk 11; S3 2; PokéAPI -14 (adds 14); source glue 0 each. See COUNTS.md.
New concepts: per-test concord_test::Origin (CONNECT + throw-away CA), client()/configure(builder)/ca(); no new DSL words.
C1: declarations are byte-identical; no test-infrastructure settings enter provider facts.
C2: fixture routing stays in Rust; S3 endpoint selection/signing and provider fakes remain Rust.
C3: production/simple declarations keep their shape; Origin adoption is optional, though migrated control tests grow.
C4: proxy and trust roots belong explicitly to the injected reqwest client; no inferred test mode.
C5: declaration diagnostics are unchanged; TLS/fixture failures use the ordinary transport boundary.
C6: inject origin.client(); configure(builder) preserves an SDK's custom transport choices.
C7: caller reqwest client, prepare(), into_parts(), send_raw() and real deployment base setters remain available.
C8: per-test proxy/CA, no system trust or environment mutation, upstream DNS, forwarding, or Concord runtime actor.
C9: all 105 existing test names retained; four new origin/CA/signed-host/link checks authored, unrun by instruction.
Simple control changed: tests and dev-dependency only; PokéAPI's declaration, production source and caller navigation are unchanged.
Does the declaration read better than before? No: it is identical; the benefit is testing its real HTTPS origin.
New caller/spec tests add 55/63/35 code lines; proxy implementation is additional infrastructure, not a claimed saving.
