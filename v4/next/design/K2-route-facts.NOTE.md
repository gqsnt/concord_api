Mastodon: 5 net glue lines removed by hand (9 table lines → 2 link lines + 2 forwarding-constant lines).
Discord REST: 1 net glue line removed by hand (11 key/struct lines → 10 route/label-adapter lines).
PokeAPI: 0 glue lines removed; production file unchanged.
New concepts: Route/RouteInfo/RequestFacts, ROUTE/ROUTES, request_facts(), declared_query_keys(), Operation::route(), matches()/literal_path(), composition().
C1: No declaration words added; all facts derive from the existing provider description.
C2: Typed static facts replace repeated route bookkeeping; limiter and URL-validation behavior stay in Rust.
C3: PokeAPI's declaration and ordinary caller retain their shape; fact/snapshot tests are optional.
C4: Resolved paths and keys are explicit; dynamic query keys stay unknown; static routes do not follow rebasing.
C5: Accessor collisions diagnose both authored origins; required compiler fixtures are listed in DESIGN.md.
C6: One generated route table per client; short accessor docs; no composition policy to learn.
C7: Caller reqwest client, prepare(), into_parts() and send_raw() remain available.
C8: Constant inspection runs no initializer, codec, Display or QueryPairs code; no new runtime actor.
C9: Oracle, canary and snapshot test designs supplied; no compilation performed; core acceptance gaps listed.
Simple control changed? No production change; only a caller example and optional generated-fact tests/snapshot added.
Does the declaration read better than before? No: it is identical; generated reference material improves, for only 6 net glue lines saved.
