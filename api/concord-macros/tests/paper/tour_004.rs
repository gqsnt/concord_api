GET ids(puuid: Puuid, start?: u32, count?: u32) -> Vec<MatchId> {
    path "by-puuid/{puuid}/ids";
    query { start; count; }
    paginate by_offset_limit(start, count);
}
