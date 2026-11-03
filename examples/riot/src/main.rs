//! `concord-example-riot`: a player summary from the live Riot API, read-only and bounded.
//!
//! ```text
//! RIOT_API_KEY=… cargo run -p concord-example-riot -- live <gameName> <tagLine> <accountRoute> <platform> <matchRoute> [matches]
//! ```
//!
//! Without `live` it only prints this usage: the example is meant to be run offline through its
//! tests (`cargo test -p concord-example-riot`). Live mode needs an explicit key in the
//! environment (never on the command line or in a URL), makes at most `4 + 5` GET requests,
//! stops on the first `429` and reports Riot's `Retry-After`, never follows a redirect (the key
//! would go with it), and never calls the tournament stub.

use concord_example_riot::{
    AccountRoute, MAX_MATCHES, MatchRoute, Platform, Riot, RiotConfig, Stop, live_http_client,
    player_summary,
};

const USAGE: &str = "usage: RIOT_API_KEY=<key> concord-example-riot live <gameName> <tagLine> <accountRoute: americas|asia|europe> <platform: euw1|na1|kr|…> <matchRoute: americas|asia|europe|sea> [matches (≤ 5)]\nThe tests run everything offline: cargo test -p concord-example-riot";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("live") || args.len() < 6 {
        println!("{USAGE}");
        return;
    }
    let Ok(key) = std::env::var("RIOT_API_KEY") else {
        eprintln!("RIOT_API_KEY is not set; live mode needs a key you supply.\n{USAGE}");
        std::process::exit(2);
    };
    let (Some(account_route), Some(platform), Some(match_route)) = (
        AccountRoute::from_label(&args[3]),
        Platform::from_label(&args[4]),
        MatchRoute::from_label(&args[5]),
    ) else {
        eprintln!("unknown route.\n{USAGE}");
        std::process::exit(2);
    };
    let matches = args
        .get(6)
        .and_then(|n| n.parse().ok())
        .unwrap_or(3)
        .min(MAX_MATCHES);

    // HTTPS only, no redirects followed (the key would follow them), a 10-second timeout.
    let http = live_http_client().expect("HTTP client");
    let riot = Riot::new(http, RiotConfig { key });
    match player_summary(
        &riot,
        &args[1],
        &args[2],
        account_route,
        platform,
        match_route,
        matches,
    )
    .await
    {
        Ok(summary) => {
            let name = summary.account.game_name.as_deref().unwrap_or("?");
            let tag = summary.account.tag_line.as_deref().unwrap_or("?");
            println!("{name}#{tag}: level {}", summary.summoner.summoner_level);
            for m in &summary.top_masteries {
                println!(
                    "  champion {}: level {}, {} points",
                    m.champion_id, m.champion_level, m.champion_points
                );
            }
            for game in &summary.recent {
                let result = if game.win { "win" } else { "loss" };
                println!(
                    "  {} {} {}: {}/{}/{} {result}",
                    game.id, game.mode, game.champion, game.kills, game.deaths, game.assists
                );
            }
        }
        Err(Stop::RateLimited { retry_after }) => {
            eprintln!(
                "rate limited by Riot (429); retry after {} s. Stopping.",
                retry_after.map_or("?".to_owned(), |s| s.to_string())
            );
            std::process::exit(1);
        }
        Err(Stop::Failed(error)) => {
            // Concord's error output names the endpoint and the failure, never the URL or the key.
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}
