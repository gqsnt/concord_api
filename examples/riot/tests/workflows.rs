//! The application workflows end to end against fixtures: Riot ID → PUUID → profile, masteries
//! and a bounded match history; and the tournament-stub body call.

mod support;

use std::collections::BTreeSet;

use concord_example_riot::*;
use support::{ALLY, PLAYER, riot};
use support::{Reply, Server, fixture};

fn json(name: &str) -> Reply {
    Reply::new(200)
        .header("content-type", "application/json;charset=utf-8")
        .body(fixture(name))
}

fn routed() -> impl Fn(&support::Recorded) -> Reply + Send + Sync + 'static {
    |r| {
        let path = r.path();
        if path.contains("/by-riot-id/") {
            json("account.json")
        } else if path.contains("/summoners/") {
            json("summoner.json")
        } else if path.ends_with("/top") {
            json("masteries_top.json")
        } else if path.ends_with("/ids") {
            json("match_ids.json")
        } else if let Some(id) = path.strip_prefix("/lol/match/v5/matches/") {
            json(&format!("match_{id}.json"))
        } else {
            Reply::new(404)
        }
    }
}

#[tokio::test]
async fn player_summary_reads_a_bounded_history() {
    let server = Server::start(routed()).await;
    let summary = player_summary(
        &riot(&server),
        "Fixture Player",
        "EUW",
        AccountRoute::Europe,
        Platform::Euw1,
        MatchRoute::Europe,
        2,
    )
    .await
    .unwrap();
    assert_eq!(summary.account.puuid, Puuid(PLAYER.into()));
    assert_eq!(summary.summoner.summoner_level, 287);
    assert_eq!(summary.top_masteries.len(), 3);
    assert_eq!(
        summary.recent,
        [
            MatchLine {
                id: MatchId("EUW1_7000000002".into()),
                mode: "CLASSIC".into(),
                champion: "Ahri".into(),
                kills: 7,
                deaths: 2,
                assists: 9,
                win: true,
            },
            MatchLine {
                id: MatchId("EUW1_7000000001".into()),
                mode: "ARAM".into(),
                champion: "LeeSin".into(),
                kills: 3,
                deaths: 8,
                assists: 11,
                win: false,
            },
        ]
    );
    // 4 + 2 requests, each on its own route.
    let hosts: Vec<String> = server
        .requests()
        .iter()
        .map(|r| {
            r.header("host")
                .unwrap()
                .rsplit_once(':')
                .unwrap()
                .0
                .to_owned()
        })
        .collect();
    assert_eq!(
        hosts,
        [
            "europe.api.riotgames.com",
            "euw1.api.riotgames.com",
            "euw1.api.riotgames.com",
            "europe.api.riotgames.com",
            "europe.api.riotgames.com",
            "europe.api.riotgames.com",
        ]
    );
}

#[tokio::test]
async fn history_is_capped_whatever_the_caller_asks() {
    let server = Server::start(routed()).await;
    player_summary(
        &riot(&server),
        "Fixture Player",
        "EUW",
        AccountRoute::Europe,
        Platform::Euw1,
        MatchRoute::Europe,
        1_000,
    )
    .await
    .unwrap();
    let ids = &server.requests()[3];
    assert_eq!(ids.query(), Some(&*format!("count={MAX_MATCHES}")));
}

#[tokio::test]
async fn tournament_codes_workflow() {
    let server = Server::reply(json("codes.json")).await;
    let parameters = TournamentCodeParameters {
        allowed_participants: Some(BTreeSet::from([Puuid(PLAYER.into()), Puuid(ALLY.into())])),
        metadata: None,
        team_size: 5,
        pick_type: PickType::BlindPick,
        map_type: MapType::HowlingAbyss,
        spectator_type: SpectatorType::None,
        enough_players: false,
    };
    let codes = tournament_codes(
        &riot(&server),
        AccountRoute::Americas,
        TournamentId(9),
        parameters,
        2,
    )
    .await
    .unwrap();
    assert_eq!(codes.len(), 2);
    let r = server.single();
    let body: serde_json::Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "allowedParticipants": [PLAYER, ALLY],
            "teamSize": 5,
            "pickType": "BLIND_PICK",
            "mapType": "HOWLING_ABYSS",
            "spectatorType": "NONE",
            "enoughPlayers": false
        })
    );
    assert_eq!(r.query(), Some("tournamentId=9&count=2"));
}
