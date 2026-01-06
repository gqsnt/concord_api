use concord_core::prelude::{ApiClientError, DebugLevel};

mod riot;
mod test_api;
#[cfg(doctest)]
mod ui_doc;

pub async fn test_riot() -> Result<(), ApiClientError> {
    use riot::*;
    // API key via env (ne pas hardcoder)
    dotenvy::dotenv().ok();
    let api_key = dotenvy::var("RIOT_API_KEY").expect("RIOT_API_KEY missing");

    // Riot API (auth + routing)
    let riot = riot_client::RiotClient::new(api_key);

    let _default_platform = PlatformRoute::EUW1;
    let default_region = RegionalRoute::Europe;
    // Exemple: RiotID -> AccountDto -> PUUID
    // NOTE: choisir le bon RegionalRoute en fonction du shard du joueur.
    let account = riot
        .execute(riot_client::endpoints::GetAccountByRiotId::new(
            default_region,
            "Random Iron",
            "EUVV",
        ))
        .await?;
    println!(
        "Account: {}#{} puuid={}",
        account.game_name, account.tag_line, account.puuid
    );

    // // Exemple: PUUID -> SummonerDto (platform routing)
    // // NOTE: choisir PlatformRoute cohérent.
    // let riot = riot::riot_api::RiotClient::with_vars({
    //     riot::riot_api::RiotClientVars::new(std::env::var("RIOT_API_KEY").unwrap())
    // });
    //
    // let summoner = riot
    //     .execute(riot_api::endpoints::GetSummonerByPuuid::new(
    //         default_platform,
    //         account.puuid.clone(),
    //     ))
    //     .await?;
    // println!(
    //     "Summoner: name={} level={} id={}",
    //     summoner.name, summoner.summoner_level, summoner.id
    // );

    // Match IDs (match-v5)
    let match_ids: Vec<String> = riot
        .collect_all_items(
            riot_client::endpoints::GetMatchIdsByPuuid::new(default_region, account.puuid.clone())
                .count(5),
        )
        .max_items(10_000)
        .await?;
    println!("match_ids Len: {:?}", match_ids.len());
    //
    // // Data Dragon
    // let dd = ddragon::DDragonClient::new();
    // let versions = dd.execute(ddragon::endpoints::GetVersions::new()).await?;
    // let latest = versions
    //     .first()
    //     .cloned()
    //     .unwrap_or_else(|| "0.0.0".to_string());
    // println!("DDragon latest version={}", latest);
    //
    // let champs = dd
    //     .execute(
    //         ddragon::endpoints::GetChampionList::new(latest.clone()).locale("fr_FR".to_string()),
    //     )
    //     .await?;
    // println!("champions count={}", champs.data.len());
    //
    // let spells = dd
    //     .execute(ddragon::endpoints::GetSummonerSpells::new(latest).locale("fr_FR".to_string()))
    //     .await?;
    // println!("summoner spells count={}", spells.data.len());

    Ok(())
}

pub async fn test_api() -> Result<(), ApiClientError> {
    let client = test_api::client::Client::new(true);

    let posts = client
        .execute(
            test_api::client::endpoints::GetPosts::new()
                .user_id(1)
                .x_debug(true)
                .with_debug_level(DebugLevel::VV),
        )
        .await?;
    println!("GET /posts?userId=1 => {} posts", posts.len());

    let post = client
        .execute(test_api::client::endpoints::GetPost::new(1).with_debug_level(DebugLevel::V))
        .await?;
    println!("GET /posts/1 => title={:?}", post.title);

    let comments = client
        .execute(test_api::client::endpoints::GetPostComments::new(1))
        .await?;
    println!("GET /posts/1/comments => {} comments", comments.len());
    let created = client
        .execute(test_api::client::endpoints::CreatePost::new(
            test_api::models::NewPost {
                title: "foo".to_string(),
                body: "bar".to_string(),
                user_id: 10,
            },
        ))
        .await?;
    println!(
        "POST /posts => id={} user_id={}",
        created.id, created.user_id
    );

    let user = client
        .execute(test_api::client::endpoints::GetUser::new(1))
        .await?;
    println!("GET /users/1 => username={}", user.username);

    let titles = client
        .execute(test_api::client::endpoints::GetUserPosts::new(1))
        .await?;
    println!("GET /users/1/posts => {} titles (mapped)", titles.len());

    Ok(())
}

pub mod prelude {
    pub use crate::riot::{
        LeagueQueue, PlatformRoute, RegionalRoute, d_dragon_client, riot_client,
    };
    pub use crate::test_riot;
}
