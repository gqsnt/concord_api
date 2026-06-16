use concord_examples::riot::{PlatformRoute, RegionalRoute, RiotClient};
use concord_test_support::mock;
use std::path::PathBuf;

#[test]
fn riot_like_large_fixture_snapshot_is_bounded() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();
    let source = std::fs::read_to_string(workspace.join("concord_examples/src/riot.rs"))
        .expect("read riot fixture");
    let snapshot = std::fs::read_to_string("tests/snapshots/riot_large_surface.snap")
        .expect("read snapshot")
        .replace("\r\n", "\n");

    assert_eq!(
        snapshot,
        "riot_fixture_summary:\nclients: RiotClient, DDragonClient\nfeatures: nested_scopes, aliases, path_query_header_fmt, auth, rate_limit_profiles, offset_pagination\nfacade_paths: platform(...).summoner_v4().by_puuid, regional(...).match_v5_matches().ids_by_puuid().paginate, ddragon(...).cdn_versioned(...).data_localized(...).champion().get_champion_list\n"
    );
    for required in [
        "client RiotClient",
        "scope platform(platform: PlatformRoute)",
        "scope regional(region: RegionalRoute)",
        "as ids_by_puuid",
        "\"X-Riot-Puuid\" = fmt[\"puuid:\", puuid]",
        "auth header \"X-Riot-Token\" = riot_api_key",
        "rate_limit match_v5_method",
        "paginate OffsetLimitPagination",
    ] {
        assert!(
            source.contains(required),
            "riot fixture missing required large-API fragment `{required}`"
        );
    }
}

#[test]
fn riot_like_large_fixture_facade_paths_typecheck_cleanly() {
    let (transport, handle) = mock().build();
    let riot = RiotClient::new_with_transport("riot-secret".to_string(), transport);

    let _summoner = riot
        .platform(PlatformRoute::EUW1)
        .summoner_v4()
        .by_puuid("puuid".to_string());
    let _match_ids = riot
        .regional(RegionalRoute::Europe)
        .match_v5_matches()
        .ids_by_puuid("puuid".to_string())
        .paginate()
        .max_items(10);

    handle.finish();
}

#[test]
fn riot_endpoints_do_not_place_policy_after_response() {
    let source = include_str!("../src/riot.rs");
    let mut previous_response_line = false;

    for line in source.lines() {
        let trimmed = line.trim_start();

        if previous_response_line {
            assert!(
                !trimmed.starts_with("rate_limit ")
                    && !trimmed.starts_with("retry ")
                    && !trimmed.starts_with("cache ")
                    && !trimmed.starts_with("auth "),
                "policy clause appears immediately after endpoint response line: {trimmed}"
            );
        }

        previous_response_line = trimmed.starts_with("-> ");
    }
}
