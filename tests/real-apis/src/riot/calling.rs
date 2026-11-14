use concord::Secret;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A player by Riot ID, then their ranks on their platform.
pub async fn ranks() -> Result<(), BoxError> {
    let riot = Riot::new(std::env::var("RIOT_API_KEY")?).with_retry(retry());
    let Some(account) = riot.account(Region::Asia).by_riot_id("Hide on bush", "KR1").await? else {
        println!("no such Riot ID");
        return Ok(());
    };
    for entry in riot.league(Platform::Kr).by_puuid(&account.puuid).await? {
        println!("{} LP", entry.league_points);               // no entry: unranked
    }
    Ok(())
}

/// Every ranked solo game of a player since a date, from the region of their platform.
pub async fn season(riot: &Riot, puuid: &Puuid, since: i64) -> riot::Result<i64> {
    let matches = riot.matches(Platform::Euw1.region());
    let mut ids = matches.ids(puuid).queue(420).start_time(since).count(100).pages().items();
    let mut seconds = 0;
    while let Some(id) = ids.try_next().await? {
        if let Some(game) = matches.get(&id).await? {        // `None`: Riot no longer keeps it
            seconds += game.info.game_duration;
        }
    }
    Ok(seconds)
}

/// A whole division of the ladder, page after page.
pub async fn division(riot: &Riot) -> riot::Result<()> {
    let mut players = riot.league(Platform::Euw1)
        .entries(Queue::RankedSolo, Tier::Diamond, Division::I)
        .pages()
        .items();
    while let Some(entry) = players.try_next().await? {
        println!("{:?} {} LP", entry.puuid, entry.league_points);
    }
    Ok(())
}

/// Is the player in a game now? A `404` is an answer; a key that expired is an error.
pub async fn in_game(riot: &Riot, puuid: &Puuid) -> riot::Result<()> {
    match riot.spectator(Platform::Kr).active_game(puuid).await {
        Ok(Some(game)) => println!("in a game for {} s", game.game_length),
        Ok(None) => println!("not in a game"),
        Err(e) if e.is_status(403) => eprintln!("the key was refused: a development key lasts a day"),
        Err(e) => return Err(e),
    }
    Ok(())
}

/// The signed-in player, with the token of Riot Sign On: no API key goes on these requests.
pub async fn me(riot: &Riot, token: Secret) -> riot::Result<()> {
    let account = riot.account(Region::Americas).me(&token).await?;
    let profile = riot.summoner(Platform::Na1).me(&token).await?;
    let recent = riot.rso_matches(Region::Americas, &token).ids().count(20).send().await?;
    println!("{:?}: level {}, {} recent games", account.game_name, profile.summoner_level, recent.len());
    Ok(())
}

/// A tournament: written against the stub, run against the live API. With `retry()`, a `429`
/// on these `POST`s waits for its `Retry-After` and is retried.
pub async fn tournament(riot: &Riot, live: bool, callback: String) -> riot::Result<Vec<TournamentCode>> {
    let tournaments = riot.tournament(if live { TournamentApi::Live } else { TournamentApi::Stub });
    let provider = tournaments.register_provider(NewProvider { region: "EUW".into(), url: callback }).await?;
    let tournament = tournaments.create(NewTournament { provider_id: provider, name: "Friday cup".into() }).await?;
    tournaments.create_codes(tournament, CodeSettings {}).count(8).await
}
