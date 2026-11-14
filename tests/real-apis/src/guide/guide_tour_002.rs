concord::api! {
pub client Riot(key: Secret) {
    headers { "x-riot-token" = key; }                  // who you are, on every request below
    read Json;                                         // how this API answers
    problem RiotProblem;                               // how this API says no

    scope matches(route: MatchRoute) {
        base "https://{route}.api.riotgames.com";      // every request below
        path "lol/match/v5/matches";

        GET ids(puuid: Puuid, count?: u32) -> Vec<MatchId> {
            path "by-puuid/{puuid}/ids";
            query { count; }
        }

        GET get(id: MatchId) -> Match { path "{id}"; }
    }
}
}
