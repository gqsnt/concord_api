use concord_examples::riot::{PlatformRoute, RegionalRoute, RiotClient};
use concord_test_support::mock;
use std::path::PathBuf;

fn source_contains_in_order(source: &str, snippets: &[&str]) -> bool {
    let mut search_from = 0;

    for snippet in snippets {
        let Some(relative) = source[search_from..].find(snippet) else {
            return false;
        };
        search_from += relative + snippet.len();
    }

    true
}

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

#[test]
fn riot_uses_default_riot_read_behavior() {
    let source = include_str!("../src/riot.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "behaviors {",
            "behavior riot_read {",
            "auth header \"X-Riot-Token\" = riot_api_key",
            "retry read",
            "rate_limit app",
            "defaults {",
            "behavior riot_read",
        ],
    ));

    assert_eq!(
        source
            .matches("auth header \"X-Riot-Token\" = riot_api_key")
            .count(),
        1,
        "X-Riot-Token auth should be declared once in behavior riot_read"
    );
}

#[test]
fn riot_groups_secret_and_credential_auth_config() {
    let source = include_str!("../src/riot.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "auth {",
            "secret api_key: String",
            "credential riot_api_key = api_key(secret.api_key)",
            "behaviors {",
            "behavior riot_read {",
            "auth header \"X-Riot-Token\" = riot_api_key",
        ],
    ));
}

#[test]
fn riot_groups_policy_profiles() {
    let source = include_str!("../src/riot.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "policies {",
            "retry read {",
            "observe rate_limit RiotRateLimitHeaders",
            "rate_limit app {",
            "rate_limit match_v5_method {",
            "behaviors {",
        ],
    ));
}

#[test]
fn riot_lifts_uniform_behavior_to_scopes() {
    let source = include_str!("../src/riot.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "scope champion_masteries",
            "path [\"champion-masteries\"]",
            "behavior high_volume_read",
            "GET GetChampionMasteriesBySummoner",
            "-> Json<Vec<models::ChampionMasteryDto>>",
        ],
    ));

    assert!(source_contains_in_order(
        source,
        &[
            "scope scores",
            "path [\"scores\"]",
            "behavior high_volume_read",
            "GET GetChampionMasteryScore",
            "-> Json<i32>",
        ],
    ));

    assert!(source_contains_in_order(
        source,
        &[
            "scope challenges_v1",
            "path [\"challenges\", \"v1\"]",
            "behavior high_volume_read",
            "GET GetChallengePercentiles",
            "-> Json<serde_json::Value>",
        ],
    ));

    assert!(source_contains_in_order(
        source,
        &[
            "scope account_v1_accounts",
            "path [\"riot\", \"account\", \"v1\", \"accounts\"]",
            "behavior account_standard_read",
            "GET GetAccountByRiotId",
            "-> Json<models::AccountDto>",
        ],
    ));

    assert!(source_contains_in_order(
        source,
        &[
            "scope tournament_stub_v5",
            "path [\"lol\", \"tournament-stub\", \"v5\"]",
            "behavior high_volume_read",
            "POST CreateTournamentStubCodes",
            "-> Json<Vec<String>>",
        ],
    ));
}

#[test]
fn riot_keeps_mixed_match_v5_behaviors_explicit() {
    let source = include_str!("../src/riot.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "scope match_v5_matches",
            "GET GetMatchIdsByPuuid",
            "behavior match_v5_read",
            "-> Json<Vec<String>>",
            "GET GetMatch",
            "behavior match_v5_read",
            "-> Json<models::MatchDto>",
            "GET GetTimeline",
            "behavior match_v5_read",
            "-> Json<models::TimelineDto>",
            "GET GetMatchReplaysByPuuid",
            "behavior high_volume_read",
            "-> Json<serde_json::Value>",
        ],
    ));
}
