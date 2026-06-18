use concord_examples::ddragon::DDragonClient;
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

fn workspace_source(path: &str) -> String {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("examples crate has workspace parent")
        .to_path_buf();
    std::fs::read_to_string(workspace.join(path))
        .unwrap_or_else(|err| panic!("read workspace source {path}: {err}"))
}

fn endpoint_block<'a>(source: &'a str, endpoint: &str) -> &'a str {
    let start = source
        .find(endpoint)
        .unwrap_or_else(|| panic!("missing endpoint `{endpoint}`"));
    let rest = &source[start..];
    let end = rest
        .find("-> ")
        .map(|idx| idx + rest[idx..].find('\n').unwrap_or(rest.len() - idx))
        .unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn riot_like_large_fixture_snapshot_is_bounded() {
    let source = include_str!("../src/riot.rs");
    let snapshot = std::fs::read_to_string("tests/snapshots/riot_large_surface.snap")
        .expect("read snapshot")
        .replace("\r\n", "\n");

    assert_eq!(
        snapshot,
        "riot_fixture_summary:\nclient: RiotClient\nfeatures: platform_routing, regional_routing, auth, app_rate_limit, method_rate_limits, live_game_cache, offset_pagination\nfacade_paths: platform(...).summoner_v4().by_puuid, regional(...).match_v5_matches().ids_by_puuid().paginate, platform(...).spectator_v5().active_game_by_puuid\n"
    );
    for required in [
        "client RiotClient",
        "scope platform(platform: PlatformRoute)",
        "scope regional(region: RegionalRoute)",
        "as ids_by_puuid",
        "behavior spectator_live_game_read",
        "auth header \"X-Riot-Token\" = riot_api_key",
        "rate_limit match_v5_standard",
        "paginate OffsetLimitPagination",
    ] {
        assert!(
            source.contains(required),
            "riot fixture missing required large-API fragment `{required}`"
        );
    }
}

#[test]
fn riot_and_ddragon_are_split() {
    let riot = include_str!("../src/riot.rs");
    let ddragon = include_str!("../src/ddragon.rs");

    assert!(riot.contains("client RiotClient"));
    assert!(!riot.contains("client DDragonClient"));
    assert!(ddragon.contains("client DDragonClient"));
    assert!(!ddragon.contains("client RiotClient"));
}

#[test]
fn riot_summoner_dto_accepts_current_puuid_shape() {
    let value = serde_json::json!({
        "profileIconId": 123,
        "revisionDate": 1710000000000i64,
        "puuid": "x".repeat(78),
        "summonerLevel": 42
    });

    let dto: concord_examples::riot::models::SummonerDto =
        serde_json::from_value(value).expect("summoner dto should decode current PUUID shape");

    assert_eq!(dto.profile_icon_id, 123);
    assert_eq!(dto.revision_date, 1710000000000i64);
    assert_eq!(dto.puuid.len(), 78);
    assert_eq!(dto.summoner_level, 42);
    assert!(dto.id.is_none());
    assert!(dto.account_id.is_none());
}

#[test]
fn riot_account_dto_accepts_missing_game_name_and_tag_line() {
    let value = serde_json::json!({
        "puuid": "x".repeat(78)
    });

    let dto: concord_examples::riot::models::AccountDto =
        serde_json::from_value(value).expect("account dto should decode without Riot ID fields");

    assert_eq!(dto.puuid.len(), 78);
    assert!(dto.game_name.is_none());
    assert!(dto.tag_line.is_none());
}

#[test]
fn riot_account_dto_accepts_riot_id_fields_when_present() {
    let value = serde_json::json!({
        "puuid": "x".repeat(78),
        "gameName": "Player",
        "tagLine": "EUW"
    });

    let dto: concord_examples::riot::models::AccountDto =
        serde_json::from_value(value).expect("account dto should decode Riot ID fields");

    assert_eq!(dto.puuid.len(), 78);
    assert_eq!(dto.game_name.as_deref(), Some("Player"));
    assert_eq!(dto.tag_line.as_deref(), Some("EUW"));
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
    let _live_game = riot
        .platform(PlatformRoute::EUW1)
        .spectator_v5()
        .active_game_by_puuid("puuid".to_string());

    handle.finish();
}

#[test]
fn ddragon_fixture_facade_paths_typecheck_cleanly() {
    let (transport, handle) = mock().build();
    let ddragon = DDragonClient::new_with_transport(transport);

    let _versions = ddragon.ddragon().api().versions();
    let _champions = ddragon
        .ddragon()
        .cdn_versioned("15.1.1".to_string())
        .data_localized()
        .champion_list();
    let _champion = ddragon
        .ddragon()
        .cdn_versioned("15.1.1".to_string())
        .data_localized()
        .champion_detail("Aatrox".to_string());

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
                    && !trimmed.starts_with("auth ")
                    && !trimmed.starts_with("behavior "),
                "policy clause appears immediately after endpoint response line: {trimmed}"
            );
        }

        previous_response_line = trimmed.starts_with("-> ");
    }
}

#[test]
fn riot_uses_default_riot_read_behavior_and_single_header_auth() {
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
fn riot_app_rate_limit_is_production_value() {
    let source = include_str!("../src/riot.rs");

    assert!(source_contains_in_order(
        source,
        &[
            "rate_limit app {",
            "bucket application by [host] {",
            "500 / 10s",
            "30000 / 10m",
        ],
    ));
}

#[test]
fn riot_method_rate_limit_profiles_are_present() {
    let source = include_str!("../src/riot.rs");

    for (profile, windows) in [
        ("rate_limit summoner_by_puuid", &["1600 / 1m"][..]),
        (
            "rate_limit league_apex_by_queue",
            &["30 / 10s", "500 / 10m"],
        ),
        ("rate_limit league_entries_page", &["50 / 10s"]),
        (
            "rate_limit league_entries_by_puuid",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        ("rate_limit league_exp_entries_page", &["50 / 10s"]),
        ("rate_limit clash_team", &["200 / 1m"]),
        ("rate_limit clash_tournament", &["10 / 1m"]),
        (
            "rate_limit clash_players_by_puuid",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        ("rate_limit account_standard_lookup", &["1000 / 1m"]),
        (
            "rate_limit account_high_volume",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        (
            "rate_limit status_platform_data",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        ("rate_limit match_v5_standard", &["2000 / 10s"]),
        (
            "rate_limit match_v5_replays",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        (
            "rate_limit challenges_high_volume",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        (
            "rate_limit champion_mastery_high_volume",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        (
            "rate_limit tournament_stub_high_volume",
            &["20000 / 10s", "1200000 / 10m"],
        ),
        (
            "rate_limit spectator_live_game",
            &["3000 / 10s", "180000 / 10m"],
        ),
        (
            "rate_limit champion_rotation_high_volume",
            &["20000 / 10s", "1200000 / 10m"],
        ),
    ] {
        assert!(source.contains(profile), "missing `{profile}`");
        for window in windows {
            assert!(
                source_contains_in_order(source, &[profile, window]),
                "`{profile}` should include `{window}`"
            );
        }
    }
}

#[test]
fn riot_critical_endpoint_behavior_mapping_is_correct() {
    let source = include_str!("../src/riot.rs");

    for snippets in [
        &[
            "GET GetChampionRotations",
            "behavior champion_rotation_high_volume_read",
        ][..],
        &["GET GetSummonerByPuuid", "behavior summoner_by_puuid_read"],
        &[
            "GET GetChallengerLeagueByQueue",
            "behavior league_apex_read",
        ],
        &[
            "GET GetGrandmasterLeagueByQueue",
            "behavior league_apex_read",
        ],
        &["GET GetMasterLeagueByQueue", "behavior league_apex_read"],
        &["GET GetLeagueEntries", "behavior league_entries_page_read"],
        &[
            "GET GetLeagueEntriesByPuuid",
            "behavior league_entries_by_puuid_read",
        ],
        &[
            "GET GetLeagueExpEntries",
            "behavior league_exp_entries_page_read",
        ],
        &[
            "GET GetClashPlayersByPuuid",
            "behavior clash_players_by_puuid_read",
        ],
        &["GET GetAccountByRiotId", "behavior account_standard_read"],
        &["GET GetAccountByPuuid", "behavior account_standard_read"],
        &[
            "GET GetAccountRegionByGameAndPuuid",
            "behavior account_high_volume_read",
        ],
        &["GET GetMatchIdsByPuuid", "behavior match_v5_standard_read"],
        &["GET GetMatch", "behavior match_v5_standard_read"],
        &["GET GetTimeline", "behavior match_v5_standard_read"],
        &[
            "GET GetMatchReplaysByPuuid",
            "behavior match_v5_replays_read",
        ],
        &[
            "scope challenges_v1",
            "behavior challenges_high_volume_read",
        ],
        &[
            "scope champion_mastery_v4",
            "behavior champion_mastery_high_volume_read",
        ],
        &[
            "scope tournament_stub_v5",
            "behavior tournament_stub_high_volume_read",
        ],
        &[
            "GET GetActiveGameByPuuid",
            "behavior spectator_live_game_read",
        ],
    ] {
        assert!(
            source_contains_in_order(source, snippets),
            "missing ordered mapping {snippets:?}"
        );
    }

    let champion_rotations = endpoint_block(source, "GET GetChampionRotations");
    assert!(champion_rotations.contains("behavior champion_rotation_high_volume_read"));
    assert!(!champion_rotations.contains("behavior league_apex_read"));
    assert!(!source.contains("league_queue"));
}

#[test]
fn riot_cache_is_only_for_live_game_data() {
    let source = include_str!("../src/riot.rs");
    let ddragon = include_str!("../src/ddragon.rs");

    assert!(source_contains_in_order(
        source,
        &["cache live_game_1m {", "http", "ttl 60s"]
    ));
    assert!(source_contains_in_order(
        source,
        &[
            "behavior spectator_live_game_read {",
            "rate_limit spectator_live_game",
            "cache live_game_1m",
        ],
    ));

    for behavior in source.split("behavior ").skip(1) {
        if behavior.starts_with("spectator_live_game_read") {
            continue;
        }
        assert!(
            !behavior.contains("cache "),
            "only spectator_live_game_read should attach cache"
        );
    }
    assert!(!ddragon.contains("cache "));
}

#[test]
fn riot_does_not_expose_stale_endpoints() {
    let source = include_str!("../src/riot.rs");

    for stale in [
        "by-name",
        "GetSummonerByName",
        "GetSummonerById",
        "GetChampionMasteriesBySummoner",
        "path [\"champion-masteries\", \"by-summoner\"",
        "path [\"scores\", \"by-summoner\"",
        "GetChampionMasteryScore(summoner_id",
        "GetLeagueById",
    ] {
        assert!(
            !source.contains(stale),
            "stale Riot endpoint `{stale}` remains"
        );
    }
}

#[test]
fn ddragon_host_base_and_required_endpoints_are_present() {
    let source = include_str!("../src/ddragon.rs");
    let snapshot = std::fs::read_to_string("tests/snapshots/ddragon_surface.snap")
        .expect("read ddragon snapshot")
        .replace("\r\n", "\n");

    assert_eq!(
        snapshot,
        "ddragon_fixture_summary:\nclient: DDragonClient\nfeatures: ddragon_host, versions, realms, localized_data\nfacade_paths: ddragon().api().versions, ddragon().cdn_versioned(...).data_localized().champion_list\n"
    );

    assert!(source.contains("base \"https://leagueoflegends.com\""));
    assert!(source.contains("host [\"ddragon\"]"));

    for required in [
        "versions.json",
        "languages.json",
        "path [\"realms\", fmt[region, \".json\"]]",
        "champion.json",
        "path [\"champion\", fmt[champion_id, \".json\"]]",
        "item.json",
        "summoner.json",
        "profileicon.json",
    ] {
        assert!(
            source.contains(required),
            "DDragon fixture missing `{required}`"
        );
    }
}

#[test]
fn ddragon_has_no_auth_rate_limit_or_cache() {
    let source = include_str!("../src/ddragon.rs");

    assert!(!source.contains("auth "));
    assert!(!source.contains("rate_limit "));
    assert!(!source.contains("cache "));
}

#[test]
fn source_files_are_split_on_disk() {
    let riot = workspace_source("concord_examples/src/riot.rs");
    let ddragon = workspace_source("concord_examples/src/ddragon.rs");

    assert!(riot.contains("client RiotClient"));
    assert!(!riot.contains("client DDragonClient"));
    assert!(ddragon.contains("client DDragonClient"));
    assert!(!ddragon.contains("client RiotClient"));
}
