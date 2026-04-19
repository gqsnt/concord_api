use concord_core::prelude::*;
use concord_examples::riot::{LeagueQueue, PlatformRoute, RegionalRoute};
use concord_examples::riot::{RiotClient, riot_endpoints as endpoints};
use concord_test_support::mock;
use http::Method;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
struct RecordedPlan {
    endpoint: &'static str,
    method: Method,
    url: String,
    plan: RateLimitPlan,
}

#[derive(Clone, Default)]
struct DenyRecordingLimiter {
    plans: Arc<Mutex<Vec<RecordedPlan>>>,
}

impl RateLimiter for DenyRecordingLimiter {
    fn acquire<'a>(
        &'a self,
        ctx: RateLimitContext<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<RateLimitPermit, ApiClientError>> + Send + 'a>> {
        Box::pin(async move {
            self.plans.lock().expect("plans lock").push(RecordedPlan {
                endpoint: ctx.endpoint,
                method: ctx.method.clone(),
                url: ctx.url.to_string(),
                plan: ctx.plan.clone(),
            });
            Err(ApiClientError::PolicyViolation {
                ctx: ErrorContext {
                    endpoint: ctx.endpoint,
                    method: ctx.method.clone(),
                },
                msg: "test limiter denied request",
            })
        })
    }
}

type Window = (u32, u64);

fn method_windows(plan: &RateLimitPlan) -> Vec<Vec<Window>> {
    let mut windows = plan
        .buckets()
        .iter()
        .filter(|bucket| bucket.id.kind == "method")
        .map(|bucket| {
            bucket
                .windows
                .iter()
                .map(|window| (window.max.get(), window.per.as_secs()))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    windows.sort();
    windows
}

fn application_windows(plan: &RateLimitPlan) -> Vec<Vec<Window>> {
    plan.buckets()
        .iter()
        .filter(|bucket| bucket.id.kind == "application")
        .map(|bucket| {
            bucket
                .windows
                .iter()
                .map(|window| (window.max.get(), window.per.as_secs()))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn assert_windows(
    recorded: &RecordedPlan,
    method: Method,
    host: &str,
    path: &str,
    expected_method_windows: &[&[Window]],
) {
    assert_eq!(recorded.method, method, "{}", recorded.endpoint);

    let url = url::Url::parse(&recorded.url).expect("recorded URL should parse");
    assert_eq!(url.host_str(), Some(host), "{}", recorded.endpoint);
    assert_eq!(url.path(), path, "{}", recorded.endpoint);

    assert_eq!(
        application_windows(&recorded.plan),
        vec![vec![(500, 10), (30000, 600)]],
        "{}",
        recorded.endpoint
    );

    let mut expected = expected_method_windows
        .iter()
        .map(|windows| windows.to_vec())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(
        method_windows(&recorded.plan),
        expected,
        "{}",
        recorded.endpoint
    );
}

async fn record<T: std::fmt::Debug>(
    plans: &Arc<Mutex<Vec<RecordedPlan>>>,
    request: impl Future<Output = Result<T, ApiClientError>>,
) -> RecordedPlan {
    let index = plans.lock().expect("plans lock").len();
    request
        .await
        .expect_err("test limiter should deny before transport");
    plans
        .lock()
        .expect("plans lock")
        .get(index)
        .expect("limiter should record a plan")
        .clone()
}

#[tokio::test]
async fn riot_platform_methods_have_declared_rate_limits() {
    let (transport, handle) = mock().build();
    let limiter = DenyRecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RiotClient::new_with_transport("riot-key".to_string(), transport)
        .with_rate_limiter(Arc::new(limiter));

    let platform = PlatformRoute::EUW1;
    let host = "euw1.api.riotgames.com";
    let slow = &[(30, 10), (500, 600)];
    let by_id = &[(500, 10)];
    let entries = &[(50, 10)];
    let summoner = &[(1600, 60)];
    let clash_200 = &[(200, 60)];
    let clash_10 = &[(10, 60)];
    let high = &[(20000, 10), (1200000, 600)];

    let cases = [
        (
            record(
                &plans,
                api.request(endpoints::platform::champion_v3::GetChampionRotations::new(platform))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/platform/v3/champion-rotations",
            vec![slow.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::summoner_v4::GetSummonerByPuuid::new(
                    platform,
                    "puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/summoner/v4/summoners/by-puuid/puuid",
            vec![summoner.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::league_v4::challengerleagues::GetChallengerLeagueByQueue::new(
                    platform,
                    LeagueQueue::RankedSolo5x5,
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/league/v4/challengerleagues/by-queue/RANKED_SOLO_5x5",
            vec![slow.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::league_v4::grandmasterleagues::GetGrandmasterLeagueByQueue::new(
                    platform,
                    LeagueQueue::RankedSolo5x5,
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/league/v4/grandmasterleagues/by-queue/RANKED_SOLO_5x5",
            vec![slow.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::league_v4::masterleagues::GetMasterLeagueByQueue::new(
                    platform,
                    LeagueQueue::RankedSolo5x5,
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/league/v4/masterleagues/by-queue/RANKED_SOLO_5x5",
            vec![slow.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::league_v4::leagues::GetLeagueById::new(
                    platform,
                    "league-id".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/league/v4/leagues/league-id",
            vec![by_id.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::league_v4::entries::GetLeagueEntries::new(
                    platform,
                    "RANKED_SOLO_5x5".to_string(),
                    "DIAMOND".to_string(),
                    "I".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/league/v4/entries/RANKED_SOLO_5x5/DIAMOND/I",
            vec![entries.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::league_v4::entries::GetLeagueEntriesByPuuid::new(
                    platform,
                    "encrypted-puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/league/v4/entries/by-puuid/encrypted-puuid",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::league_exp_v4::GetLeagueExpEntries::new(
                    platform,
                    "RANKED_SOLO_5x5".to_string(),
                    "DIAMOND".to_string(),
                    "I".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/league-exp/v4/entries/RANKED_SOLO_5x5/DIAMOND/I",
            vec![entries.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::clash_v1::GetClashTeam::new(
                    platform,
                    "team-id".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/clash/v1/teams/team-id",
            vec![clash_200.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::clash_v1::GetClashTournament::new(platform, 42))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/clash/v1/tournaments/42",
            vec![clash_10.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::clash_v1::GetClashTournamentByTeam::new(
                    platform,
                    "team-id".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/clash/v1/tournaments/by-team/team-id",
            vec![clash_200.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::clash_v1::GetClashTournaments::new(platform))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/clash/v1/tournaments",
            vec![clash_10.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::clash_v1::GetClashPlayersByPuuid::new(
                    platform,
                    "puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/clash/v1/players/by-puuid/puuid",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::challenges_v1::GetChallengePercentiles::new(platform))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/challenges/v1/challenges/percentiles",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::challenges_v1::GetChallengeLeaderboardsByLevel::new(
                    platform,
                    1,
                    "MASTER".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/challenges/v1/challenges/1/leaderboards/by-level/MASTER",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::challenges_v1::GetChallengePercentilesByChallenge::new(
                    platform, 1,
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/challenges/v1/challenges/1/percentiles",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::challenges_v1::GetChallengeConfig::new(platform, 1))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/challenges/v1/challenges/1/config",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::challenges_v1::GetChallengePlayerData::new(
                    platform,
                    "puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/challenges/v1/player-data/puuid",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::challenges_v1::GetChallengeConfigs::new(platform))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/challenges/v1/challenges/config",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::champion_mastery_v4::champion_masteries::GetChampionMasteriesByPuuid::new(
                    platform,
                    "encrypted-puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/champion-mastery/v4/champion-masteries/by-puuid/encrypted-puuid",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::champion_mastery_v4::champion_masteries::GetChampionMasteryByPuuidAndChampion::new(
                    platform,
                    "encrypted-puuid".to_string(),
                    266,
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/champion-mastery/v4/champion-masteries/by-puuid/encrypted-puuid/by-champion/266",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::champion_mastery_v4::scores::GetChampionMasteryScoreByPuuid::new(
                    platform,
                    "encrypted-puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/champion-mastery/v4/scores/by-puuid/encrypted-puuid",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::champion_mastery_v4::champion_masteries::GetTopChampionMasteriesByPuuid::new(
                    platform,
                    "encrypted-puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/champion-mastery/v4/champion-masteries/by-puuid/encrypted-puuid/top",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::status_v4::GetPlatformData::new(platform))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/status/v4/platform-data",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::platform::spectator_v5::GetSpectatorV5ActiveGameBySummoner::new(
                    platform,
                    "encrypted-puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/spectator/v5/active-games/by-summoner/encrypted-puuid",
            vec![high.as_slice()],
        ),
    ];

    for (recorded, method, path, windows) in cases {
        assert_windows(&recorded, method, host, path, &windows);
    }

    assert_eq!(handle.recorded_len(), 0);
    handle.finish();
}

#[tokio::test]
async fn riot_regional_methods_have_declared_rate_limits() {
    let (transport, handle) = mock().build();
    let limiter = DenyRecordingLimiter::default();
    let plans = limiter.plans.clone();
    let api = RiotClient::new_with_transport("riot-key".to_string(), transport)
        .with_rate_limiter(Arc::new(limiter));

    let region = RegionalRoute::Europe;
    let host = "europe.api.riotgames.com";
    let account = &[(1000, 60)];
    let high = &[(20000, 10), (1200000, 600)];
    let match_v5 = &[(2000, 10)];
    let body = serde_json::json!({});

    let cases = [
        (
            record(
                &plans,
                api.request(endpoints::regional::account_v1_accounts::GetAccountByRiotId::new(
                    region,
                    "game".to_string(),
                    "tag".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/riot/account/v1/accounts/by-riot-id/game/tag",
            vec![account.as_slice(), high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::account_v1_accounts::GetAccountByPuuid::new(
                    region,
                    "puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/riot/account/v1/accounts/by-puuid/puuid",
            vec![account.as_slice(), high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::account_v1_region::GetAccountRegionByGameAndPuuid::new(
                    region,
                    "lol".to_string(),
                    "puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/riot/account/v1/region/by-game/lol/by-puuid/puuid",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::match_v5_matches::GetMatch::new(region, "match-id".to_string()))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/match/v5/matches/match-id",
            vec![match_v5.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::match_v5_matches::GetMatchIdsByPuuid::new(
                    region,
                    "puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/match/v5/matches/by-puuid/puuid/ids",
            vec![match_v5.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::match_v5_matches::GetTimeline::new(region, "match-id".to_string()))
                    .execute(),
            )
            .await,
            Method::GET,
            "/lol/match/v5/matches/match-id/timeline",
            vec![match_v5.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::match_v5_matches::GetMatchReplaysByPuuid::new(
                    region,
                    "puuid".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/match/v5/matches/by-puuid/puuid/replays",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::tournament_stub_v5::CreateTournamentStubCodes::new(
                    region,
                    1,
                    body.clone(),
                ))
                .execute(),
            )
            .await,
            Method::POST,
            "/lol/tournament-stub/v5/codes",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::tournament_stub_v5::GetTournamentStubLobbyEventsByCode::new(
                    region,
                    "code".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/tournament-stub/v5/lobby-events/by-code/code",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::tournament_stub_v5::GetTournamentStubCode::new(
                    region,
                    "code".to_string(),
                ))
                .execute(),
            )
            .await,
            Method::GET,
            "/lol/tournament-stub/v5/codes/code",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::tournament_stub_v5::RegisterTournamentStubProvider::new(
                    region,
                    body.clone(),
                ))
                .execute(),
            )
            .await,
            Method::POST,
            "/lol/tournament-stub/v5/providers",
            vec![high.as_slice()],
        ),
        (
            record(
                &plans,
                api.request(endpoints::regional::tournament_stub_v5::RegisterTournamentStubTournament::new(
                    region, body,
                ))
                .execute(),
            )
            .await,
            Method::POST,
            "/lol/tournament-stub/v5/tournaments",
            vec![high.as_slice()],
        ),
    ];

    for (recorded, method, path, windows) in cases {
        assert_windows(&recorded, method, host, path, &windows);
    }

    assert_eq!(handle.recorded_len(), 0);
    handle.finish();
}
