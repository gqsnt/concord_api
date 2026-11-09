Overlay: `<sdk>/` mirrors `examples/<sdk>-sdk/` in rv-p1; unlisted files stay unchanged; copied unchanged files show the declarations and retained glue.
Glue removed, counted by hand from deleted nonblank SDK source: mastodon 0; quickbooks-online 0; nager-holidays 0; no modules shrink or disappear (408/500/82 classified glue lines remain).
New concept: `mut` on required client/scope values (C1 variant a); optional/initialized values keep setters, and all endpoint setters keep today's rules.
C1: `mut connection` states that reauthorization can replace the complete realm-bound connection; required customer/invoice scope IDs identify fixed selections.
C2: Rotation, refresh and recovery remain Rust; Mastodon's private wrapper/forwarders and QuickBooks' connect/host-selection policy remain necessary without the separate value-backed-base and accessor features.
C3: Simple control unchanged: Nager's src/lib.rs is byte-identical; no new client/config/scope concept reaches its callers.
C4: Required client/scope `x: T` has no setter; `mut x: T` has one; `?`/initializers visibly retain the existing setters; mut preserves snapshot semantics.
C5: Retargeting fixed customer/invoice scope IDs fails with E0599 at the caller's set_id; select another scope with customer(other)/invoice(other).
C6: One sentence suffices: required client/scope values are fixed unless mut; optional/initialized values and endpoint values keep their setters.
C7: The supplied reqwest client, prepare(), into_parts(), send_raw() and existing with_base_url escape hatches remain available; Mastodon's private client stays private.
C8: No new runtime actor or implicit rotation; existing Rust policies still run explicitly on send(), including QuickBooks' token-free prepare/base-snapshot contract.
C9: Added future positive/compile-fail doctests for set_connection, required config and both absent set_id methods; retained wire/canary suites; no compilation or test execution performed.
Existing SDK tests needing changes: none; QuickBooks tests/environment.rs and policy.rs environment_tests retain coordinated realm/credentials/host replacement and fixture behavior; Mastodon/Nager suites remain satisfiable.
Zendesk: write `mut policy: SendPolicy` and keep supplying policy in ZendeskConfig; set_policy survives without `?` or a default, preserving model.rs:583's required-send-policy rule (rotating required authorization likewise needs mut).
Does the declaration read better than before? Yes, narrowly: mut connection makes replacement explicit with one word; Mastodon and Nager read exactly as before, and no glue saving is claimed.
