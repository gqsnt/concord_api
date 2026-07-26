//! Runtime composition and the explicitly manual live Riot workflow.
//!
//! Required features: `json` and `rate-limit`; `riot-live` for the
//! binary. Mode: deterministic construction plus explicit manual live access.
//! Documentation owner: `docs/recipes/riot.md`.
//! Proving tests: `tests/construction.rs` and `tests/riot_contract.rs`.

use std::error::Error;
use std::num::NonZeroU32;
use std::sync::Arc;

use concord::hooks::SanitizedHeaderValue;
use concord::http::HeaderName;
use concord::prelude::PageLimit;
use concord::rate_limit::{
    Cooldown, CooldownScope, LocalRateLimiter, RateLimitObserver, RateLimitResponseHead,
    RetryAfterObserver,
};

use super::api::{DDragonApi, RiotApi, d_dragon_api, riot_api};
use super::domain::MatchType;

/// Illustratively interprets final `429` response headers for future acquisitions.
///
/// Static Concord buckets pace ordinary bursts. This example lets
/// `Retry-After` and `X-Rate-Limit-Type` refine a cooldown after a rejection;
/// it is not a verified statement of Riot's current header contract. Method
/// cooldowns conservatively cool the current lexical plan.
#[derive(Default)]
pub struct RiotRateLimitObserver;

impl RateLimitObserver for RiotRateLimitObserver {
    fn observe(&self, response: &RateLimitResponseHead<'_>) -> Option<Cooldown> {
        let default = RetryAfterObserver.observe(response)?;
        let rate_limit_type = HeaderName::from_static("x-rate-limit-type");

        let scope = match response.headers.get(&rate_limit_type) {
            Some(SanitizedHeaderValue::Text(value))
                if value.eq_ignore_ascii_case("application") =>
            {
                CooldownScope::Host
            }
            Some(SanitizedHeaderValue::Text(value)) if value.eq_ignore_ascii_case("service") => {
                CooldownScope::Host
            }
            Some(SanitizedHeaderValue::Text(value)) if value.eq_ignore_ascii_case("method") => {
                CooldownScope::CurrentBuckets
            }
            _ => return Some(default),
        };

        Some(Cooldown {
            duration: default.duration,
            scope,
        })
    }
}

/// Constructs representative platform, regional, paginated, and Data Dragon requests.
pub fn construct() -> Result<(), concord::ClientBuildError> {
    let riot = RiotApi::builder()
        .api_key(
            concord::SecretString::try_new("manual-example-key")
                .expect("bounded manual example key"),
        )
        .build()?;
    let _: riot_api::platform::summoner_v4::SummonerByPuuidRequest = riot
        .platform()
        .summoner_v4()
        .summoner_by_puuid("example-puuid".to_owned());
    let _: concord::pagination::PaginatedRequest<_> = riot
        .regional()
        .match_v5()
        .match_ids_by_puuid("example-puuid".to_owned())
        .set_match_type(MatchType::Ranked)
        .set_count(NonZeroU32::new(10).expect("positive page size"))
        .paginate(PageLimit::Take(2));

    let data_dragon = DDragonApi::builder().build()?;
    let _: d_dragon_api::VersionsRequest = data_dragon.versions();
    let _: d_dragon_api::version::data::ChampionsRequest =
        data_dragon.version("15.1.1".to_owned()).data().champions();
    Ok(())
}

/// Runs the live Riot workflow from `RIOT_API_KEY`, `RIOT_GAME_NAME`, and
/// `RIOT_TAG_LINE`.
///
/// This function is called only by the `riot` binary behind the `riot-live`
/// feature and is never executed by the deterministic test suite.
pub async fn run_live_from_environment() -> Result<(), Box<dyn Error>> {
    let api_key = std::env::var("RIOT_API_KEY")?;
    let game_name = std::env::var("RIOT_GAME_NAME")?;
    let tag_line = std::env::var("RIOT_TAG_LINE")?;

    let limiter = LocalRateLimiter::new().observer(Arc::new(RiotRateLimitObserver));
    let riot = RiotApi::builder()
        .api_key(concord::SecretString::try_from(api_key)?)
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(limiter)))
        .build()?;

    let account = riot
        .regional()
        .account_v1()
        .account_by_riot_id(game_name, tag_line)
        .await?;

    println!(
        "Riot ID: {}#{}\nPUUID: {}",
        account.game_name.as_deref().unwrap_or("<unknown>"),
        account.tag_line.as_deref().unwrap_or("<unknown>"),
        account.puuid
    );

    let summoner = riot
        .platform()
        .summoner_v4()
        .summoner_by_puuid(account.puuid.clone())
        .await?;

    println!(
        "Summoner level: {} | profile icon: {}",
        summoner.summoner_level, summoner.profile_icon_id
    );

    let league_entries = riot
        .platform()
        .league_v4()
        .league_entries_by_puuid(account.puuid.clone())
        .await?;

    for entry in &league_entries {
        println!(
            "{}: {} {} — {} LP ({}W / {}L)",
            entry.queue_type, entry.tier, entry.rank, entry.league_points, entry.wins, entry.losses
        );
    }

    let top_masteries = riot
        .platform()
        .champion_mastery_v4()
        .top_masteries_by_puuid(account.puuid.clone())
        .set_count(5)
        .await?;

    println!("Top champion masteries: {}", top_masteries.len());

    let match_ids = riot
        .regional()
        .match_v5()
        .match_ids_by_puuid(account.puuid.clone())
        .set_match_type(MatchType::Ranked)
        .set_count(NonZeroU32::new(20).expect("20 is non-zero"))
        .paginate(PageLimit::Take(2))
        .await?;

    println!("Collected {} recent ranked match IDs", match_ids.len());

    if let Some(match_id) = match_ids.first() {
        let response = riot
            .regional()
            .match_v5()
            .match_by_id(match_id.clone())
            .response()
            .await?;

        println!(
            "Match {}: HTTP {} | queue={} | duration={}s | participants={}",
            response.value().metadata.match_id,
            response.status(),
            response.value().info.queue_id,
            response.value().info.game_duration,
            response.value().info.participants.len()
        );

        let timeline = riot
            .regional()
            .match_v5()
            .timeline_by_id(match_id.clone())
            .await?;

        println!("Timeline frames: {}", timeline.info.frames.len());
    }

    match riot
        .platform()
        .spectator_v5()
        .active_game_by_puuid(account.puuid.clone())
        .await
    {
        Ok(game) => println!(
            "Live game found: id={} mode={} participants={}",
            game.game_id,
            game.game_mode,
            game.participants.len()
        ),
        Err(error) => println!("No accessible live game: {error}"),
    }

    let status = riot
        .platform()
        .status_v4()
        .platform_data()
        .response()
        .await?;
    println!(
        "{} status: HTTP {}, incidents={}, maintenances={}",
        status.value().name,
        status.status(),
        status.value().incidents.len(),
        status.value().maintenances.len()
    );

    Ok(())
}
