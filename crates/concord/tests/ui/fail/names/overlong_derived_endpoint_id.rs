// V3-L16 / IDENTITY-01 (§12.4) and CALL-01 (§7.1): without an explicit `id`,
// the operational endpoint ID is navigation-derived as
// `snake_case(Client).scope_path.endpoint_name`. It must be ASCII, at most
// 128 bytes, in dot-separated segments of at most 63 bytes. An invalid or
// overlong derivation is rejected with a `[C3-NAME]` diagnostic that asks for
// an explicit ID; it is never truncated, hashed, transliterated or suffixed.
// One invocation per case because each closed declaration reports its first
// violation. `tests/ui/pass/language/explicit_id_fixes_overlong_derivation.rs`
// is the matching pass case (explicit IDs, and derivations at the limits).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

// The endpoint segment is 64 bytes.
api! {
    client SegmentIds {
        base "https://ids.example.test/";

        GET this_navigation_derived_segment_runs_to_sixty_four_bytes_in_full() -> Bytes via Binary;
    }
}

// The scope segment is 64 bytes; the endpoint under it has no explicit ID.
api! {
    client ScopeIds {
        base "https://ids.example.test/";

        scope this_navigation_derived_scope_name_runs_to_sixty_four_bytes_full() {
            GET read() -> Bytes via Binary;
        }
    }
}

// Every segment is at most 63 bytes, but the whole ID is 129 bytes.
api! {
    client TotalIds {
        base "https://ids.example.test/";

        scope catalog_scope_whose_logical_name_spans_fifty_nine_bytes_now() {
            GET get_item_endpoint_whose_logical_name_spans_fifty_nine_bytes() -> Bytes via Binary;
        }
    }
}

// A Unicode endpoint identifier is valid Rust, but it has no ASCII
// operational derivation; Concord does not transliterate it.
api! {
    client UnicodeIds {
        base "https://ids.example.test/";

        GET données() -> Bytes via Binary;
    }
}

fn main() {}
