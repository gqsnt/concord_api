//! Exact requests: method, routed host, path, query, headers and body bytes, per operation.

mod support;

use std::collections::BTreeSet;

use concord_example_riot::*;
use support::{ALLY, KEY, PLAYER, ddragon, riot};
use support::{Recorded, Reply, Server, fixture};

fn json(name: &str) -> Reply {
    Reply::new(200)
        .header("content-type", "application/json;charset=utf-8")
        .body(fixture(name))
}

/// Every Riot request: the routed host, the key in `x-riot-token`, `accept`, no `authorization`.
fn riot_headers(r: &Recorded, host: &str) {
    let port_suffix = r.header("host").unwrap().rsplit_once(':').unwrap().0;
    assert_eq!(port_suffix, host, "{r:?}");
    assert_eq!(r.header("x-riot-token"), Some(KEY));
    assert_eq!(r.header("accept"), Some("application/json"));
    assert_eq!(r.header("authorization"), None);
}

#[tokio::test]
async fn account_by_riot_id_is_regional_and_path_encoded() {
    let server = Server::reply(json("account.json")).await;
    let account = riot(&server)
        .account(AccountRoute::Americas)
        .by_riot_id("Fixture Player", "EUW")
        .send()
        .await
        .unwrap();
    assert_eq!(account.puuid, Puuid(PLAYER.into()));
    assert_eq!(account.game_name.as_deref(), Some("Fixture Player"));
    let r = server.single();
    assert_eq!(r.method, "GET");
    assert_eq!(
        r.target,
        "/riot/account/v1/accounts/by-riot-id/Fixture%20Player/EUW"
    );
    riot_headers(&r, "americas.api.riotgames.com");
    assert!(r.body.is_empty() && r.header("content-type").is_none());
}

#[tokio::test]
async fn optional_account_fields_may_be_omitted() {
    let server = Server::reply(json("account_no_tag.json")).await;
    let account = riot(&server)
        .account(AccountRoute::Europe)
        .by_puuid(Puuid(ALLY.into()))
        .send()
        .await
        .unwrap();
    assert_eq!((account.game_name, account.tag_line), (None, None));
    assert_eq!(
        server.single().target,
        format!("/riot/account/v1/accounts/by-puuid/{ALLY}")
    );
}

#[tokio::test]
async fn platform_operations_use_the_platform_host() {
    let server = Server::start(|r| match r.path() {
        p if p.contains("/summoners/") => json("summoner.json"),
        p if p.ends_with("/top") => json("masteries_top.json"),
        _ => json("rotation.json"),
    })
    .await;
    let euw = riot(&server).platform(Platform::Euw1);
    let summoner = euw.summoner(Puuid(PLAYER.into())).send().await.unwrap();
    assert_eq!(summoner.summoner_level, 287);
    let top = euw
        .top_masteries(Puuid(PLAYER.into()))
        .set_count(3)
        .send()
        .await
        .unwrap();
    assert_eq!(top.len(), 3);
    assert_eq!(top[1].tokens_earned, 0, "omitted zero");
    let rotation = euw.rotation().send().await.unwrap();
    assert_eq!(
        rotation.free,
        [ChampionId(1), ChampionId(22), ChampionId(51)]
    );
    let r = server.requests();
    assert_eq!(
        r[0].target,
        format!("/lol/summoner/v4/summoners/by-puuid/{PLAYER}")
    );
    assert_eq!(
        r[1].target,
        format!("/lol/champion-mastery/v4/champion-masteries/by-puuid/{PLAYER}/top?count=3")
    );
    assert_eq!(r[2].target, "/lol/platform/v3/champion-rotations");
    for request in &r {
        riot_headers(request, "euw1.api.riotgames.com");
    }
}

#[tokio::test]
async fn match_ids_send_only_the_filters_that_are_set() {
    let server = Server::reply(json("match_ids.json")).await;
    let matches = riot(&server).matches(MatchRoute::Europe);
    let ids = matches.ids(Puuid(PLAYER.into())).send().await.unwrap();
    assert_eq!(ids.len(), 2);
    matches
        .ids(Puuid(PLAYER.into()))
        .set_start_time(1_700_000_000)
        .set_queue(420)
        .set_kind(MatchType::Ranked)
        .set_start(0)
        .set_count(2)
        .send()
        .await
        .unwrap();
    let r = server.requests();
    assert_eq!(
        r[0].target,
        format!("/lol/match/v5/matches/by-puuid/{PLAYER}/ids")
    );
    assert_eq!(
        r[1].query(),
        Some("startTime=1700000000&queue=420&type=ranked&start=0&count=2")
    );
    riot_headers(&r[1], "europe.api.riotgames.com");
}

#[tokio::test]
async fn tournament_codes_send_the_documented_body() {
    let server = Server::reply(json("codes.json")).await;
    let parameters = TournamentCodeParameters {
        allowed_participants: Some(BTreeSet::from([Puuid(PLAYER.into()), Puuid(ALLY.into())])),
        metadata: Some("synthetic-final".into()),
        team_size: 5,
        pick_type: PickType::TournamentDraft,
        map_type: MapType::SummonersRift,
        spectator_type: SpectatorType::LobbyOnly,
        enough_players: true,
    };
    let codes = riot(&server)
        .tournament_stub(AccountRoute::Americas)
        .create_codes(TournamentId(2002), parameters)
        .set_count(2)
        .send()
        .await
        .unwrap();
    assert_eq!(
        codes,
        [
            TournamentCode::from("SYNTH-CODE-0001"),
            TournamentCode::from("SYNTH-CODE-0002")
        ]
    );
    let r = server.single();
    assert_eq!(r.method, "POST");
    assert_eq!(
        r.target,
        "/lol/tournament-stub/v5/codes?tournamentId=2002&count=2"
    );
    riot_headers(&r, "americas.api.riotgames.com");
    assert_eq!(r.header("content-type"), Some("application/json"));
    let expected = format!(
        "{{\"allowedParticipants\":[\"{PLAYER}\",\"{ALLY}\"],\"metadata\":\"synthetic-final\",\"teamSize\":5,\"pickType\":\"TOURNAMENT_DRAFT\",\"mapType\":\"SUMMONERS_RIFT\",\"spectatorType\":\"LOBBYONLY\",\"enoughPlayers\":true}}"
    );
    assert_eq!(String::from_utf8(r.body).unwrap(), expected);
}

#[tokio::test]
async fn optional_tournament_fields_are_omitted_not_null() {
    let server = Server::start(|r| {
        if r.path().ends_with("/providers") {
            json("provider_id.json")
        } else {
            json("tournament_id.json")
        }
    })
    .await;
    let stub = riot(&server).tournament_stub(AccountRoute::Americas);
    let provider = stub
        .register_provider(ProviderRegistration {
            region: TournamentRegion::Euw,
            url: "https://callback.example.invalid/results".into(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(provider, ProviderId(1001));
    let tournament = stub
        .register_tournament(TournamentRegistration {
            provider_id: provider,
            name: None,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(tournament, TournamentId(2002));
    let r = server.requests();
    assert_eq!(
        r[0].body,
        br#"{"region":"EUW","url":"https://callback.example.invalid/results"}"#
    );
    assert_eq!(r[1].body, br#"{"providerId":1001}"#);
}

#[tokio::test]
async fn rso_access_token_replaces_the_api_key() {
    let server = Server::reply(json("account.json")).await;
    riot(&server)
        .account(AccountRoute::Europe)
        .me("synthetic-access-token")
        .send()
        .await
        .unwrap();
    let r = server.single();
    assert_eq!(r.target, "/riot/account/v1/accounts/me");
    assert_eq!(
        r.header("authorization"),
        Some("Bearer synthetic-access-token")
    );
    assert_eq!(r.header("x-riot-token"), None);
}

#[tokio::test]
async fn data_dragon_is_credential_free_json_and_bytes() {
    let server = Server::start(|r| {
        if r.path().ends_with(".png") {
            Reply::new(200)
                .header("content-type", "image/png")
                .body(fixture("Ahri.png"))
        } else {
            json("ddragon_versions.json")
        }
    })
    .await;
    let dd = ddragon(&server);
    let versions = dd.versions().send().await.unwrap();
    assert_eq!(versions[0], "16.21.1");
    let png = dd
        .champion_square(versions[0].clone(), "Ahri")
        .send()
        .await
        .unwrap();
    assert_eq!(&png[..], &fixture("Ahri.png")[..]);
    let r = server.requests();
    assert_eq!(r[0].target, "/api/versions.json");
    assert_eq!(r[1].target, "/cdn/16.21.1/img/champion/Ahri.png");
    for request in &r {
        assert!(
            request
                .header("host")
                .unwrap()
                .starts_with("ddragon.leagueoflegends.com")
        );
        // The Riot key never reaches the static-data host.
        assert_eq!(request.header("x-riot-token"), None);
        assert_eq!(request.header("authorization"), None);
    }
}
