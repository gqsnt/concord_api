use concord_examples::ddragon::DDragonClient;
use concord_examples::riot::{PlatformRoute, RegionalRoute, RiotClient};
use concord_test_support::mock;

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
