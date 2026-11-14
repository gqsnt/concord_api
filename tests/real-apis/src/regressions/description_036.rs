concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


GET ids(puuid: Puuid, start?: u32, count?: u32) -> Vec<MatchId> {
    path "by-puuid/{puuid}/ids";
    query { start; count; }
    paginate by_offset_limit(start, count);
}


} }