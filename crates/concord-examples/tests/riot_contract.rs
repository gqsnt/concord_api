//! Deterministic planning and contract proofs for the Riot and Data Dragon
//! declarations: host composition, private API-key placement, additive rate
//! plans and their static coverage, optional/default controls, explicit
//! traversal terminals, derived identity and representation provenance.
//!
//! No test contacts a network: plans and descriptions are inert.
#![forbid(unsafe_code)]
#![cfg(all(feature = "json", feature = "rate-limit", feature = "default-tls"))]

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use concord::endpoint::TimeoutSelection;
use concord::http::{HeaderName, Method};
use concord::inspection::{
    ChallengeRule, CredentialSourceKind, HeadFamily, IdSource, InitValue, PlacementTarget,
    RepresentationSource, TimeoutSource,
};
use concord::pagination::{CollectAll, CollectPrefix, Pages};
use concord::rate_limit::{LocalRateLimiter, NoopRateLimiter, RateDimensionKind};
use concord::{RequestControls, SecretString};
use concord_examples::riot::api::{DDragonApi, RiotApi};
use concord_examples::riot::domain::{
    DDragonLocale, MatchType, PlatformRoute, ProviderRegistrationParameters, RegionalRoute,
    TournamentRegistrationParameters,
};

const API_KEY_SENTINEL: &str = "riot-api-key-sentinel-phase11-a91f";

fn client() -> RiotApi {
    RiotApi::builder()
        .api_key(SecretString::try_new(API_KEY_SENTINEL).expect("bounded Riot test key"))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(LocalRateLimiter::new())))
        .build()
        .expect("Riot client")
}

#[test]
fn platform_and_regional_plans_compose_hosts_limits_and_redacted_auth() {
    let client = client();
    let platform = RiotApi::unbound()
        .platform(PlatformRoute::Euw1)
        .summoner_v4()
        .summoner_by_puuid("public-puuid".to_owned());
    let platform_plan = client.plan(&platform).expect("platform plan");
    assert!(
        platform_plan
            .observed_url()
            .starts_with("https://euw1.api.riotgames.com/"),
        "{}",
        platform_plan.observed_url()
    );
    assert_eq!(platform_plan.rate_limit_policy_count(), 2);

    let regional = RiotApi::unbound()
        .regional(RegionalRoute::Europe)
        .account_v1()
        .account_by_riot_id("Example Name".to_owned(), "TAG".to_owned());
    let regional_plan = client.plan(&regional).expect("regional plan");
    assert!(
        regional_plan
            .observed_url()
            .starts_with("https://europe.api.riotgames.com/"),
        "{}",
        regional_plan.observed_url()
    );
    assert_eq!(regional_plan.rate_limit_policy_count(), 2);

    let token_header = HeaderName::from_static("x-riot-token");
    for plan in [&platform_plan, &regional_plan] {
        assert!(plan.public_headers().get(&token_header).is_none());
        assert!(plan.capabilities().authentication());
        let safe_surfaces = format!(
            "{}\n{:?}\n{:?}",
            plan.observed_url(),
            plan.public_headers(),
            plan
        );
        assert!(!safe_surfaces.contains(API_KEY_SENTINEL));
        // The bound plan names the actual enforcement choice (EXPLAIN-04).
        assert_eq!(
            plan.rate_limit().enforcement_label(),
            "declared policies; enforced by the application-selected process-local limiter"
        );
    }

    // The private placement is a declared header target of the API-key
    // source, never an ordinary public value (AUTH-01, HEAD-01).
    let contract = platform
        .description()
        .contract()
        .expect("generated contract");
    let placements = contract.auth().placements();
    assert_eq!(placements.len(), 1);
    assert_eq!(
        placements[0].placement().target(),
        PlacementTarget::Header("X-Riot-Token")
    );
    assert_eq!(placements[0].placement().credential(), "riot_api_key");
    assert_eq!(
        placements[0].placement().challenge_rule(),
        ChallengeRule::Never
    );
    assert_eq!(placements[0].source_kind(), CredentialSourceKind::ApiKey);
    assert!(!placements[0].automatic_recovery_available());
}

#[test]
fn optional_defaults_pagination_binding_and_explicit_description_are_predictable() {
    let client = client();
    let default_endpoint = RiotApi::unbound()
        .regional(RegionalRoute::Europe)
        .match_v5()
        .match_ids_by_puuid("public-puuid".to_owned());
    let default_plan = client.plan(&default_endpoint).expect("default query plan");
    // Public page-controlled values remain visible in plans. The initial
    // start/count values are 0 and DEFAULT_MATCH_PAGE_SIZE (20), while
    // absent optionals emit no key (PRIVACY-01).
    let url = default_plan.observed_url();
    assert!(
        url.ends_with("/lol/match/v5/matches/by-puuid/public-puuid/ids?start=0&count=20"),
        "{url}"
    );
    assert!(!url.contains("startTime="), "{url}");
    assert!(!url.contains("endTime="), "{url}");
    assert!(!url.contains("queue="), "{url}");
    assert!(!url.contains("type="), "{url}");

    let endpoint = RiotApi::unbound()
        .regional(RegionalRoute::Europe)
        .match_v5()
        .match_ids_by_puuid("public-puuid".to_owned())
        .set_match_type(MatchType::Ranked)
        .set_start(40)
        .set_count(NonZeroU32::new(10).expect("positive page size"));
    let description = endpoint.description();
    assert_eq!(description.method(), Method::GET);
    assert!(
        description
            .source_path()
            .ends_with("regional::match_v5::match_ids_by_puuid"),
        "{}",
        description.source_path()
    );
    assert!(description.capabilities().authentication());
    assert!(description.capabilities().rate_limits());
    assert!(description.capabilities().pagination());

    let plan = client.plan(&endpoint).expect("configured query plan");
    assert!(
        plan.observed_url()
            .ends_with("/ids?type=ranked&start=40&count=10"),
        "{}",
        plan.observed_url()
    );
    assert_eq!(plan.endpoint_id(), description.id());
    assert_eq!(
        plan.endpoint_id(),
        "riot_api.regional.match_v5.match_ids_by_puuid"
    );
    assert_eq!(plan.rate_limit_policy_count(), 2);
    assert!(plan.capabilities().pagination());

    // Exact outer-slot replacement.
    let replaced = endpoint
        .clone()
        .replace_match_type(None)
        .replace_queue(Some(420))
        .replace_start_time(Some(100));
    let replaced_plan = client.plan(&replaced).expect("replaced plan");
    assert!(
        replaced_plan
            .observed_url()
            .ends_with("/ids?startTime=100&queue=420&start=40&count=10"),
        "{}",
        replaced_plan.observed_url()
    );
    let reset = replaced.reset_start().reset_count().omit_start_time();
    assert!(
        client
            .plan(&reset)
            .expect("reset plan")
            .observed_url()
            .ends_with("/ids?queue=420&start=0&count=20")
    );

    // Ordinary await returns one page; traversal needs an explicit terminal.
    let _prefix: CollectPrefix<String> = client.request(endpoint.clone()).collect_prefix(2);
    let _complete: CollectAll<String> = client
        .request(endpoint.clone())
        .collect_all(NonZeroU32::new(5).expect("positive"));
    let _pages: Pages<String> = client.request(endpoint.clone()).pages(3);
    let pagination = description
        .contract()
        .expect("contract")
        .pagination()
        .expect("pagination declaration");
    assert_eq!(
        pagination.controller(),
        "concord::pagination::OffsetPagination"
    );
    assert_eq!(pagination.init_type(), "concord::pagination::OffsetInit");
    let init = pagination
        .init()
        .iter()
        .map(|field| {
            let InitValue::Value(value) = field.value() else {
                panic!("init fields bind endpoint values");
            };
            (field.field(), value.name())
        })
        .collect::<Vec<_>>();
    assert_eq!(init, [("offset", "start"), ("page_size", "count")]);
    let cursor = pagination
        .cursor_destinations()
        .iter()
        .map(|value| value.name())
        .collect::<Vec<_>>();
    assert_eq!(cursor, ["start"]);
    assert!(!format!("{description:?}\n{plan:?}").contains(API_KEY_SENTINEL));
}

#[test]
fn query_programs_keep_provider_key_spelling_and_order() {
    let client = client();
    let request = client
        .regional(RegionalRoute::Americas)
        .match_v5()
        .match_ids_by_puuid("puuid-a")
        .set_start_time(100)
        .set_end_time(200)
        .set_queue(420)
        .set_match_type(MatchType::Ranked);
    let url = request.plan().expect("plan").observed_url().to_owned();
    // Provider camel-case spelling and query occurrence order are exact.
    assert!(
        url.ends_with("/ids?startTime=100&endTime=200&queue=420&type=ranked&start=0&count=20"),
        "{url}"
    );
    let contract = request.description().contract().expect("contract");
    let keys = contract
        .operations(HeadFamily::Query)
        .iter()
        .map(|operation| operation.operation().key())
        .collect::<Vec<_>>();
    assert_eq!(
        keys,
        ["startTime", "endTime", "queue", "type", "start", "count"]
    );

    let codes = client
        .regional(RegionalRoute::Europe)
        .tournament_stub_v5()
        .create_stub_codes(9, Default::default())
        .set_count(2)
        .plan()
        .expect("plan");
    assert!(
        codes
            .observed_url()
            .ends_with("/lol/tournament-stub/v5/codes?tournamentId=9&count=2"),
        "{}",
        codes.observed_url()
    );
    let entries = client
        .platform(PlatformRoute::Na1)
        .league_v4()
        .league_entries(
            concord_examples::riot::domain::LeagueQueue::RankedSolo5x5,
            concord_examples::riot::domain::LeagueTier::Gold,
            concord_examples::riot::domain::LeagueDivision::Ii,
        )
        .plan()
        .expect("plan");
    assert!(
        entries
            .observed_url()
            .ends_with("/lol/league/v4/entries/RANKED_SOLO_5x5/GOLD/II?page=1"),
        "{}",
        entries.observed_url()
    );
    let headers = format!("{:?}", entries.public_headers());
    assert!(headers.contains("user-agent"), "{headers}");
}

/// Static coverage (RATE-02): the three originally scope-shared groups keep
/// one scope-level attachment covering their endpoints; application buckets
/// cover their whole route tree; per-endpoint IDs cover one endpoint each.
#[test]
fn static_policy_coverage_keeps_the_original_sharing_groups() {
    let contract = RiotApi::contract();
    assert_eq!(contract.endpoints().len(), 46);
    assert_eq!(contract.policies().len(), 28);
    let covered = |id: &str| -> BTreeSet<&str> {
        contract
            .policy(id)
            .unwrap_or_else(|| panic!("policy {id}"))
            .covered()
            .iter()
            .map(|endpoint| endpoint.path().rsplit("::").next().expect("name"))
            .collect()
    };
    let set = |names: &[&'static str]| names.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(covered("concord.riot.1").len(), 27);
    assert_eq!(covered("concord.riot.20").len(), 19);
    assert_eq!(
        covered("concord.riot.2"),
        set(&[
            "masteries_by_puuid",
            "mastery_by_champion",
            "top_masteries_by_puuid",
            "mastery_score_by_puuid"
        ])
    );
    assert_eq!(
        covered("concord.riot.15"),
        set(&[
            "challenge_configs",
            "challenge_percentiles",
            "challenge_config",
            "challenge_leaderboard",
            "challenge_percentiles_by_id",
            "challenge_player_data"
        ])
    );
    assert_eq!(
        covered("concord.riot.28"),
        set(&[
            "create_stub_codes",
            "stub_code",
            "stub_lobby_events",
            "register_stub_provider",
            "register_stub_tournament"
        ])
    );
    for (id, scope) in [
        ("concord.riot.1", "platform"),
        ("concord.riot.2", "champion_mastery_v4"),
        ("concord.riot.15", "challenges_v1"),
        ("concord.riot.20", "regional"),
        ("concord.riot.28", "tournament_stub_v5"),
    ] {
        let coverage = contract.policy(id).expect("policy");
        let sites = coverage
            .attachments()
            .iter()
            .map(|site| site.owner())
            .collect::<Vec<_>>();
        assert_eq!(sites.len(), 1, "{id}: one attachment site");
        assert!(sites[0].ends_with(scope), "{id}: attached at {}", sites[0]);
        let dimensions = coverage
            .dimensions()
            .iter()
            .map(|dimension| (dimension.label(), dimension.kind()))
            .collect::<Vec<_>>();
        assert_eq!(dimensions, [("origin", RateDimensionKind::Origin)], "{id}");
        for group in coverage.sharing() {
            assert!(
                group.condition().same_origin(),
                "{id}: origin partitions state"
            );
            assert!(!group.condition().same_endpoint(), "{id}: endpoints share");
        }
    }
    for id in (3..=27).filter(|n| ![15, 20].contains(n)) {
        let id = format!("concord.riot.{id}");
        assert_eq!(covered(&id).len(), 1, "{id} covers one endpoint");
    }
    // The tournament_v5 subtree carries only the regional application bucket.
    for endpoint in contract
        .endpoints()
        .iter()
        .filter(|endpoint| endpoint.path().contains("::tournament_v5::"))
    {
        let ids = endpoint
            .policies()
            .iter()
            .map(|policy| policy.id())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["concord.riot.20"], "{}", endpoint.path());
    }
}

/// §12.1 / V3-R03 (static half): the showcase factors equal numbers into
/// window sets, yet each attachment keeps its own policy ID and coverage, so
/// equal numbers never merge state. `tests/baseline_recording.rs` proves the
/// dynamic half on the actual generated keys.
#[test]
fn window_sets_factor_numbers_without_sharing_state() {
    let contract = RiotApi::contract();
    let sets = contract
        .window_sets()
        .iter()
        .map(|set| (set.name(), set))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(sets.len(), 10);
    assert!(
        sets.values().all(|set| !set.used_by().is_empty()),
        "no unused set"
    );
    let standard = sets["standard"];
    let windows = standard
        .windows()
        .iter()
        .map(|window| (window.capacity(), window.duration()))
        .collect::<Vec<_>>();
    assert_eq!(
        windows,
        [
            (20_000, Duration::from_secs(10)),
            (1_200_000, Duration::from_secs(600))
        ]
    );
    let users = standard.used_by().iter().copied().collect::<BTreeSet<_>>();
    let expected = [2, 3, 4, 13, 15, 16, 23, 25, 28]
        .map(|n| format!("concord.riot.{n}"))
        .into_iter()
        .collect::<BTreeSet<_>>();
    assert_eq!(
        users,
        expected.iter().map(String::as_str).collect::<BTreeSet<_>>()
    );
    let mut covered = BTreeSet::new();
    for id in users {
        let policy = contract.policy(id).expect("policy");
        assert_eq!(policy.window_set(), Some("standard"), "{id}");
        for endpoint in policy.covered() {
            // Disjoint coverage: equal numbers under different IDs never
            // cover the same endpoint twice or merge into one identity.
            assert!(covered.insert((id, endpoint.path())), "{id}");
        }
    }
    assert_eq!(
        sets["application"].used_by(),
        ["concord.riot.1", "concord.riot.20"]
    );
}

#[test]
fn every_endpoint_keeps_timeout_identity_and_auth_placement() {
    let contract = RiotApi::contract();
    for endpoint in contract.endpoints() {
        let path = endpoint.path();
        assert_eq!(endpoint.identity().source(), IdSource::Derived, "{path}");
        assert!(endpoint.id().starts_with("riot_api."), "{path}");
        assert_eq!(
            endpoint.timeout().duration(),
            Duration::from_secs(20),
            "{path}"
        );
        assert_eq!(
            endpoint.timeout().source(),
            TimeoutSource::Declared("RiotApi"),
            "{path}"
        );
        let placements = endpoint.auth().placements();
        assert_eq!(placements.len(), 1, "{path}");
        assert_eq!(
            placements[0].placement().target(),
            PlacementTarget::Header("X-Riot-Token"),
            "{path}"
        );
        assert!(endpoint.auth().displaced().is_empty(), "{path}");
    }
    let plan = client()
        .platform(PlatformRoute::Kr)
        .status_v4()
        .platform_data()
        .plan()
        .expect("plan");
    assert_eq!(
        plan.timeout_selection(),
        TimeoutSelection::Explicit(Duration::from_secs(20))
    );
}

#[test]
fn requests_are_inert_owned_and_carry_common_controls() {
    let client = client();
    let request = client
        .platform(PlatformRoute::Euw1)
        .champion_mastery_v4()
        .top_masteries_by_puuid("puuid-a")
        .set_count(5);
    let clone = request.clone();
    // The generated named request implements the open common-controls
    // trait itself (CALL-02), so generic helpers need no native access.
    fn capped<R: RequestControls>(request: R) -> R {
        request.with_options(&concord::CallOptions::new().cap_sends(1))
    }
    let request = capped(
        request
            .timeout(Duration::from_secs(5))
            .expect("finite timeout"),
    );
    let summary = request.control_summary();
    assert!(format!("{summary:?}").contains("5s"), "{summary:?}");
    assert!(
        clone
            .plan()
            .expect("plan")
            .observed_url()
            .ends_with("/top?count=5")
    );
    // Body-bearing requests are not Clone (VALUE-05); constructing one is
    // still inert.
    let _register = client
        .regional(RegionalRoute::Europe)
        .tournament_v5()
        .register_tournament(TournamentRegistrationParameters::default());

    // An explicit no-op limiter is disclosed as unenforced (V3-R12).
    let noop = RiotApi::builder()
        .api_key(SecretString::try_new(API_KEY_SENTINEL).expect("key"))
        .configure_runtime(|runtime| runtime.rate_limiter(Arc::new(NoopRateLimiter::new())))
        .build()
        .expect("noop client");
    let plan = noop
        .regional(RegionalRoute::Asia)
        .match_v5()
        .match_by_id("match-a")
        .plan()
        .expect("plan");
    assert_eq!(
        plan.rate_limit().enforcement_label(),
        "declared policies; enforcement explicitly disabled by application selection"
    );
    // A client with declared policies requires an explicit limiter choice.
    let error = RiotApi::builder()
        .api_key(SecretString::try_new(API_KEY_SENTINEL).expect("key"))
        .build()
        .expect_err("limiter required");
    assert!(!format!("{error:?}").contains(API_KEY_SENTINEL));
}

#[test]
fn data_dragon_is_an_independent_importable_contract() {
    let client = DDragonApi::builder()
        .set_locale(DDragonLocale::FrFr)
        .build()
        .expect("Data Dragon client");
    let endpoint = DDragonApi::unbound()
        .version("15.1.1".to_owned())
        .data()
        .champions();
    let plan = client.plan(&endpoint).expect("Data Dragon plan");
    assert_eq!(plan.method(), Method::GET);
    assert_eq!(plan.rate_limit_policy_count(), 0);
    assert!(
        plan.observed_url()
            .contains("/cdn/15.1.1/data/fr_FR/champion.json"),
        "{}",
        plan.observed_url()
    );
    assert!(!plan.capabilities().authentication());

    let default_client = DDragonApi::new().expect("nothing is required");
    let realm = default_client.realm("euw").plan().expect("realm plan");
    assert_eq!(
        realm.observed_url(),
        "https://ddragon.leagueoflegends.com/realms/euw.json"
    );
    let versions = default_client.versions().plan().expect("versions plan");
    assert_eq!(
        versions.observed_url(),
        "https://ddragon.leagueoflegends.com/api/versions.json"
    );
    let reset = DDragonApi::builder()
        .set_locale(DDragonLocale::FrFr)
        .reset_locale()
        .build()
        .expect("reset client")
        .version("15.1.1")
        .data()
        .items()
        .plan()
        .expect("items plan");
    assert!(
        reset
            .observed_url()
            .ends_with("/cdn/15.1.1/data/en_US/item.json"),
        "{}",
        reset.observed_url()
    );
}

#[test]
fn riot_and_data_dragon_publish_representation_inheritance_provenance() {
    let inherited = RiotApi::unbound()
        .platform(PlatformRoute::Euw1)
        .summoner_v4()
        .summoner_by_puuid("public-puuid".to_owned())
        .description();
    let inherited = inherited.contract().expect("inherited Riot details");
    assert_eq!(inherited.result().representation().type_name(), "Json");
    assert_eq!(
        inherited.result().representation().source(),
        RepresentationSource::Inherited("RiotApi")
    );
    assert_eq!(inherited.result().accept(), Some("application/json"));

    let nested_response = RiotApi::unbound()
        .platform(PlatformRoute::Euw1)
        .status_v4()
        .platform_data()
        .description();
    assert_eq!(
        nested_response
            .contract()
            .expect("nested response details")
            .result()
            .representation()
            .source(),
        RepresentationSource::Inherited("RiotApi::platform::status_v4")
    );

    let request_override = RiotApi::unbound()
        .regional(RegionalRoute::Europe)
        .tournament_stub_v5()
        .register_stub_provider(ProviderRegistrationParameters::default())
        .description();
    let request_override = request_override
        .contract()
        .expect("request override details");
    assert_eq!(
        request_override
            .body()
            .expect("request body")
            .representation()
            .source(),
        RepresentationSource::Inherited("RiotApi::regional::tournament_stub_v5")
    );
    assert_eq!(
        request_override.result().representation().source(),
        RepresentationSource::Inherited("RiotApi")
    );

    let fenced = RiotApi::unbound()
        .regional(RegionalRoute::Europe)
        .tournament_v5()
        .register_tournament(TournamentRegistrationParameters::default())
        .description();
    let fenced = fenced.contract().expect("fenced details");
    let fenced_body = fenced.body().expect("fenced body").representation();
    assert_eq!(fenced_body.source(), RepresentationSource::Inline);
    assert_eq!(fenced_body.type_name(), "Json");
    assert_eq!(
        fenced_body.blocking_fence(),
        Some("RiotApi::regional::tournament_v5")
    );
    let fenced_result = fenced.result().representation();
    assert_eq!(fenced_result.source(), RepresentationSource::Inline);
    assert_eq!(
        fenced_result.blocking_fence(),
        Some("RiotApi::regional::tournament_v5")
    );
    let empty = RiotApi::unbound()
        .regional(RegionalRoute::Europe)
        .tournament_v5()
        .update_tournament_code("code".to_owned(), Default::default())
        .description();
    let empty = empty.contract().expect("empty result details");
    assert_eq!(empty.result().representation().type_name(), "Empty");
    assert_eq!(empty.result().accept(), None);

    let asset = DDragonApi::unbound()
        .version("15.1.1".to_owned())
        .assets()
        .champion_image("Ahri.png".to_owned())
        .description();
    let asset_details = asset.contract().expect("asset details");
    assert_eq!(
        asset_details.result().representation().type_name(),
        "Binary"
    );
    assert_eq!(
        asset_details.result().representation().source(),
        RepresentationSource::Inherited("DDragonApi::version::assets")
    );
    assert_eq!(
        asset_details.result().accept(),
        Some("application/octet-stream")
    );
}
