// V3-L16 / IDENTITY-01 (§12.4) and CALL-01 (§7.1): the pass side of
// `tests/ui/fail/names/overlong_derived_endpoint_id.rs`. An explicit `id`
// pins every navigation that has no valid ASCII derivation (a 64-byte
// endpoint or scope segment, a 129-byte whole ID, a Unicode identifier),
// while derivations exactly at the limits (a 63-byte segment, a 128-byte
// whole ID) are accepted verbatim. Generated Rust names are never truncated.
use concord::{Binary, Bytes, api};

api! {
    client LongIds {
        base "http://ids.example.test/";

        // A 64-byte endpoint segment needs an explicit ID.
        GET this_navigation_derived_segment_runs_to_sixty_four_bytes_in_full() -> Bytes via Binary {
            id "long_ids.segment";
        }

        // A 63-byte endpoint segment is derived as is.
        GET this_navigation_derived_segment_runs_to_sixty_three_bytes_total() -> Bytes via Binary;

        // A 64-byte scope segment: its endpoint needs an explicit ID.
        scope this_navigation_derived_scope_name_runs_to_sixty_four_bytes_full() {
            GET read() -> Bytes via Binary {
                id "long_ids.scope_segment.read";
            }
        }

        // 8 + 1 + 59 + 1 + 59 = 128 bytes: derived as is.
        scope catalog_scope_whose_logical_name_spans_fifty_nine_bytes_now() {
            GET get_item_endpoint_whose_logical_name_spans_fifty_nine_bytes() -> Bytes via Binary;
        }

        // 8 + 1 + 60 + 1 + 59 = 129 bytes: needs an explicit ID.
        scope catalog_scope_whose_logical_name_spans_all_sixty_bytes_today() {
            GET get_item_endpoint_whose_logical_name_spans_fifty_nine_bytes() -> Bytes via Binary {
                id "long_ids.total";
            }
        }

        // No transliteration of a Unicode identifier.
        GET données() -> Bytes via Binary {
            id "long_ids.donnees";
        }
    }
}

fn main() {
    let client = LongIds::new().expect("client");

    let segment: long_ids::ThisNavigationDerivedSegmentRunsToSixtyFourBytesInFullRequest =
        client.this_navigation_derived_segment_runs_to_sixty_four_bytes_in_full();
    assert_eq!(segment.description().id(), "long_ids.segment");

    let boundary = client.this_navigation_derived_segment_runs_to_sixty_three_bytes_total();
    assert_eq!(
        boundary.description().id(),
        "long_ids.this_navigation_derived_segment_runs_to_sixty_three_bytes_total"
    );

    let scoped = client
        .this_navigation_derived_scope_name_runs_to_sixty_four_bytes_full()
        .read();
    assert_eq!(scoped.description().id(), "long_ids.scope_segment.read");

    let at_limit = client
        .catalog_scope_whose_logical_name_spans_fifty_nine_bytes_now()
        .get_item_endpoint_whose_logical_name_spans_fifty_nine_bytes();
    let id = at_limit.description().id();
    assert_eq!(id.len(), 128);
    assert_eq!(
        id,
        "long_ids.catalog_scope_whose_logical_name_spans_fifty_nine_bytes_now.get_item_endpoint_whose_logical_name_spans_fifty_nine_bytes"
    );

    let total = client
        .catalog_scope_whose_logical_name_spans_all_sixty_bytes_today()
        .get_item_endpoint_whose_logical_name_spans_fifty_nine_bytes();
    assert_eq!(total.description().id(), "long_ids.total");

    let unicode = client.données();
    assert_eq!(unicode.description().id(), "long_ids.donnees");
    assert_eq!(unicode.description().source_path(), "LongIds::données");

    let contract = LongIds::contract();
    for id in [
        "long_ids.segment",
        "long_ids.scope_segment.read",
        "long_ids.total",
        "long_ids.donnees",
    ] {
        assert!(contract.endpoint(id).is_some(), "explicit ID `{id}` is pinned");
    }
}
