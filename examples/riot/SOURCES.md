# Sources (retrieved 2026-11-02T15:40Z)

Every operation, route list and DTO field in this example comes from Riot's official developer
pages listed below. Where official pages disagree or are silent, the conflict is recorded here,
not reconciled silently. Fixtures are synthetic; passing tests prove the implementation against
this recorded contract, not live access.

The raw downloads and their SHA-256 sums are kept with the campaign records (not in the crate).
Reference pages are rendered from
`https://developer.riotgames.com/api-details/<api>` (the data behind
`https://developer.riotgames.com/apis#<api>`).

## Operations (official API reference)

| API / operation | Method + path | Routing (reference "Select Region") | Inputs | Return |
|---|---|---|---|---|
| account-v1 getByRiotId | `GET /riot/account/v1/accounts/by-riot-id/{gameName}/{tagLine}` | AMERICAS, ASIA, EUROPE ("You can query for any account in any region") | path gameName, tagLine (required strings) | AccountDto |
| account-v1 getByPuuid | `GET /riot/account/v1/accounts/by-puuid/{puuid}` | AMERICAS, ASIA, EUROPE | path puuid | AccountDto |
| summoner-v4 getByPUUID | `GET /lol/summoner/v4/summoners/by-puuid/{encryptedPUUID}` | BR1 EUN1 EUW1 JP1 KR LA1 LA2 ME1 NA1 OC1 RU SG2 TR1 TW2 VN2 | path encryptedPUUID | SummonerDTO |
| champion-mastery-v4 getTopChampionMasteriesByPUUID | `GET /lol/champion-mastery/v4/champion-masteries/by-puuid/{encryptedPUUID}/top` | same 15 platforms as summoner-v4 | path encryptedPUUID; query `count` optional int, "defaults to 3" | List[ChampionMasteryDto] |
| match-v5 getMatchIdsByPUUID | `GET /lol/match/v5/matches/by-puuid/{puuid}/ids` | AMERICAS, ASIA, EUROPE, SEA | path puuid; query optional: startTime (long, epoch s), endTime (long), queue (int), type (ranked/normal/tourney/tutorial), start (int, "Defaults to 0"), count (int, "Defaults to 20. Valid values: 0 to 100") | List[string] |
| match-v5 getMatch | `GET /lol/match/v5/matches/{matchId}` | AMERICAS, ASIA, EUROPE (+SEA, see text) | path matchId | MatchDto |
| lol-status-v4 getPlatformData | `GET /lol/status/v4/platform-data` | BR1 EUN1 EUW1 JP1 KR LA1 LA2 ME1 NA1 OC1 **PBE1** RU SG2 TR1 VN2 (no TW2) | — | PlatformDataDto |
| champion-v3 getChampionInfo | `GET /lol/platform/v3/champion-rotations` | same 15 platforms as summoner-v4 | — | ChampionInfo |
| tournament-stub-v5 createTournamentCode | `POST /lol/tournament-stub/v5/codes` | **not stated** on the reference page (see conflicts) | query `tournamentId` required long, `count` optional int (max 1000); body TournamentCodeParametersV5 (required) | List[string] |
| tournament-stub-v5 registerProviderData | `POST /lol/tournament-stub/v5/providers` | not stated | body ProviderRegistrationParametersV5 {region, url} | int |
| tournament-stub-v5 registerTournament | `POST /lol/tournament-stub/v5/tournaments` | not stated | body TournamentRegistrationParametersV5 {providerId: int, name: string "optional"} | int |
| tournament-stub-v5 getTournamentCode | `GET /lol/tournament-stub/v5/codes/{tournamentCode}` | not stated | path tournamentCode | TournamentCodeV5DTO |

Error statuses listed for every operation above: 400, 401, 403, 404, 405, 415 (tournament only), 429, 500, 502, 503, 504.

### DTOs (fields as published)

- AccountDto: `puuid` string (78 chars); `gameName` string, "may be excluded from the response if the account doesn't have a gameName"; `tagLine` string, same note.
- SummonerDTO: `profileIconId` int, `revisionDate` long (epoch ms), `puuid` string, `summonerLevel` long. (No `id`/`accountId`/`name` in the current table.)
- ChampionMasteryDto (subset used): `puuid` string, `championId` long, `championLevel` int, `championPoints` int, `lastPlayTime` long (ms), `championPointsSinceLastLevel` long, `championPointsUntilNextLevel` long, `tokensEarned` int, `markRequiredForNextLevel` int, `championSeasonMilestone` int, `chestGranted` boolean.
- MatchDto: `metadata` MetadataDto {`dataVersion`, `matchId`, `participants` List[string]}; `info` InfoDto (subset: `gameCreation` long, `gameDuration` long (seconds after 11.20), `gameMode` string, `queueId` int, `platformId` string, `participants` List[ParticipantDto]). ParticipantDto subset: `puuid`, `riotIdGameName`, `riotIdTagline`, `championName`, `teamId` int, `kills`/`deaths`/`assists` int, `totalMinionsKilled` int, `teamPosition` string, `win` boolean.
- PlatformDataDto: `id`, `name`, `locales` List[string], `maintenances`/`incidents` List[StatusDto]; StatusDto: `id` int, `maintenance_status`, `incident_severity`, `titles` List[ContentDto], `updates` List[UpdateDto], `created_at`, `archive_at`, `updated_at`, `platforms`.
- ChampionInfo: `newplayer`/`sr` arrays of champion ids (the reference lists only these two fields).
- TournamentCodeParametersV5: `allowedParticipants` Set[string] ("Optional list of encrypted puuids"), `metadata` string ("Optional"), `teamSize` int (1–5), `pickType` (BLIND_PICK, DRAFT_MODE, ALL_RANDOM, TOURNAMENT_DRAFT), `mapType` (SUMMONERS_RIFT, HOWLING_ABYSS, LEAGUE_CLASSIC), `spectatorType` (NONE, LOBBYONLY, ALL), `enoughPlayers` boolean. Sample: `{ "allowedParticipants": {}, "enoughPlayers": true, "mapType": "", "metadata": "", "pickType": "", "spectatorType": "", "teamSize": 0 }`.
- TournamentCodeV5DTO: `code`, `lobbyName`, `metaData`, `password`, `teamSize` int, `providerId` int, `pickType`, `tournamentId` int, `id` int, `region` (BR, EUNE, EUW, JP, LAN, LAS, NA, OCE, PBE, RU, TR, KR), `map`, `participants` Set[string].

## General rules (official portal / developer-relations article)

- Portal "Response Codes": "Only 200 response codes are guaranteed to return a response body as JSON. For non-200 response codes … A response body is not guaranteed to be returned. If there is a response body, its not guaranteed to be JSON." (`docs/portal`; also the developer-relations "Developer Portal Overview" article.)
- Omission: "our APIs return only non-empty values … Any numeric field that isn't returned can be assumed to be 0 (or null as you prefer). Any list field … empty list or null. Any String field … empty string or null." → `#[serde(default)]` on numeric/list/string fields is source-supported, not schema masking.
- 429: "halt future API calls for the duration, in seconds, indicated by the Retry-After header"; `X-Rate-Limit-Type` is present only when the edge enforces the limit (service limits omit it).
- API key: the reference page offers "Query Param" or "Header Param" and recommends the header ("avoids accidentally copy/pasting your API key in public"). Development keys expire every 24 h.

## Conflicts and gaps (recorded, not reconciled)

1. **Platform list.** `docs/lol` "Platform Routing Values" lists BR1 EUN1 EUW1 JP1 KR LA1 LA2 NA1 OC1 TR1 RU PH2 SG2 TH2 TW2 VN2 (has PH2, TH2; no ME1). The per-operation reference lists have ME1 and no PH2/TH2; lol-status-v4 additionally has PBE1 and lacks TW2. The example uses per-operation lists.
2. **Regional list.** account-v1: AMERICAS/ASIA/EUROPE (no SEA); match-v5 ids: AMERICAS/ASIA/EUROPE/SEA. Account and match routing therefore get different types.
3. **Tournament-stub routing.** Neither the reference page nor `docs/lol` states a routing value for tournament-stub-v5 or tournament-v5. Community sources say regional (americas). The example takes the regional host as caller data and marks this UNVERIFIED (official).
4. **API-key header name.** The retrieved official pages do not print the header's name; `X-Riot-Token` is the owner-specified name (campaign brief) and the name in common use. Recorded as owner-specified, not verified from retrieved official text.
5. **Required fields of TournamentCodeParametersV5.** The reference marks `allowedParticipants` and `metadata` "Optional" in prose and does not mark the others; the example treats the others as required.
6. **Tournament-stub eligibility.** Which key types may call the stub was not established from retrieved sources; the example makes no live tournament call and labels eligibility unverified.

## Data Dragon (official `docs/lol`, "Data Dragon")

- Versions: `GET https://ddragon.leagueoflegends.com/api/versions.json` — "You can find all valid
  Data Dragon versions in the versions file … you should always use the most recent Data Dragon
  version for a given patch". A JSON array of version strings. No API key.
- Champion square asset: `https://ddragon.leagueoflegends.com/cdn/{version}/img/champion/{Name}.png`
  (example in the docs: `cdn/16.21.1/img/champion/Aatrox.png`). PNG bytes. No API key.
- Data Dragon is a separate, credential-free host: the example uses its own client so the Riot key
  can never reach it.


## Where each contract is tested

| Contract | Test |
|---|---|
| account-v1 by Riot ID: regional host, path encoding of `Fixture Player`, `x-riot-token`, no `Authorization` | `tests/wire.rs` `account_by_riot_id_is_regional_and_path_encoded` |
| AccountDto `gameName`/`tagLine` may be excluded | `optional_account_fields_may_be_omitted` |
| summoner-v4, champion-mastery-v4 (`count`), champion-v3 on the platform host; omitted zero fields | `platform_operations_use_the_platform_host` |
| match-v5 ids: unset filters not sent; set filters in declaration order | `match_ids_send_only_the_filters_that_are_set` |
| tournament-stub-v5 createTournamentCode: `tournamentId`/`count` query, exact JSON body, `Content-Type` | `tournament_codes_send_the_documented_body` |
| provider/tournament registration bodies; optional `name` omitted, not `null` | `optional_tournament_fields_are_omitted_not_null` |
| account-v1 `/accounts/me` with `Authorization: Bearer` and no API key (docs/lol RSO curl example) | `rso_access_token_replaces_the_api_key` |
| Data Dragon versions (JSON) and champion square (PNG bytes) on its own host, no key | `data_dragon_is_credential_free_json_and_bytes` |
| Riot ID → PUUID → summoner, masteries, bounded match history (4 + n requests, routed hosts) | `tests/workflows.rs` `player_summary_reads_a_bounded_history`, `history_is_capped_whatever_the_caller_asks` |
| Tournament codes workflow | `tournament_codes_workflow` |
| Non-200 bodies: JSON, empty, HTML, kept unread; no key in errors | `tests/errors.rs` `non_success_bodies_are_kept_unread` |
| 429: stop with `Retry-After`, no retry | `a_429_stops_the_workflow_with_retry_after`, `a_429_without_retry_after_still_stops` |
| Malformed 200 body: decode error | `malformed_success_body_is_a_decode_error` |
| Key and access token withheld from Concord's `Debug`; positive control on the wire | `the_key_and_access_tokens_are_withheld_from_debug` |
