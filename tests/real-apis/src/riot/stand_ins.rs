// The data types that the page names and does not print, cut to the fields that its code uses.

macro_rules! ids {
    ($($name:ident: $inner:ty),*) => { $(
        #[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub $inner);
    )* };
}
ids!(Puuid: String, MatchId: String, TftMatchId: String, SummonerId: String, LeagueId: String,
    ClashTeamId: String, TournamentCode: String, ChampionId: i64, ChallengeId: i64,
    ClashTournamentId: i64, TournamentId: i64, ProviderId: i64);
impl From<&str> for Puuid {
    fn from(id: &str) -> Self {
        Puuid(id.to_owned())
    }
}

macro_rules! answers {
    ($($name:ident),*) => { $( #[derive(Debug, serde::Deserialize)] pub struct $name {} )* };
}
answers!(ActiveRegion, ActiveShard, ApexPlayer, ChallengeConfig, ChampionMastery,
    ChampionRotations, ClashPlayer, ClashTeam, ClashTournament, CodeDetails, LeagueList,
    LobbyEvents, Percentiles, PlatformData, PlayerChallenges, Replays, TftLeagueEntry,
    TftLeagueList, TftMatch, Timeline, TopRated, TournamentGame, FeaturedGames);

macro_rules! words {
    ($($name:ident),*) => { $( #[derive(Clone, Debug, serde::Serialize)] pub enum $name { One } )* };
}
words!(ApexLevel, Game, MatchType, RatedQueue, TftQueue);

#[derive(Clone, Debug, serde::Serialize)]
pub enum TournamentApi {
    #[serde(rename = "tournament")]
    Live,
    #[serde(rename = "tournament-stub")]
    Stub,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Tier {
    Gold,
    Platinum,
    Diamond,
}
#[derive(Clone, Debug, serde::Serialize)]
pub enum Division {
    I,
    II,
    III,
    IV,
}

#[derive(Clone, Debug, serde::Serialize)]
pub enum Queue {
    #[serde(rename = "RANKED_SOLO_5x5")]
    RankedSolo,
    #[serde(rename = "RANKED_FLEX_SR")]
    RankedFlex,
}
#[derive(Debug, serde::Deserialize)]
pub struct Account {
    pub puuid: Puuid,
    #[serde(rename = "gameName")]
    pub game_name: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
pub struct Summoner {
    #[serde(rename = "summonerLevel")]
    pub summoner_level: i64,
}
#[derive(Debug, serde::Deserialize)]
pub struct LeagueEntry {
    pub puuid: Puuid,
    #[serde(rename = "leaguePoints")]
    pub league_points: i32,
}
#[derive(Debug, serde::Deserialize)]
pub struct Match {
    pub info: MatchInfo,
}
#[derive(Debug, serde::Deserialize)]
pub struct MatchInfo {
    #[serde(rename = "gameDuration")]
    pub game_duration: i64,
}
#[derive(Debug, serde::Deserialize)]
pub struct CurrentGame {
    #[serde(rename = "gameLength")]
    pub game_length: i64,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct NewProvider {
    pub region: String,
    pub url: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct NewTournament {
    #[serde(rename = "providerId")]
    pub provider_id: ProviderId,
    pub name: String,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct CodeSettings {}
#[derive(Clone, Debug, serde::Serialize)]
pub struct CodeUpdate {}
