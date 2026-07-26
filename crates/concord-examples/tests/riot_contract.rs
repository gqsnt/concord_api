#![forbid(unsafe_code)]
#![cfg(all(feature = "json", feature = "rate-limit", feature = "default-tls"))]

use std::num::NonZeroU32;

use concord::http::{HeaderName, Method};
use concord::pagination::PaginatedRequest;
use concord::{PageLimit, SecretString};
use concord_examples::riot::api::{DDragonApi, RiotApi, d_dragon_api, riot_api};
use concord_examples::riot::domain::{DDragonLocale, MatchType, PlatformRoute, RegionalRoute};

const API_KEY_SENTINEL: &str = "riot-api-key-sentinel-phase11-a91f";

fn client() -> RiotApi {
    RiotApi::builder()
        .api_key(SecretString::try_new(API_KEY_SENTINEL).expect("bounded Riot test key"))
        .build()
        .expect("Riot client")
}

#[test]
fn platform_and_regional_plans_compose_hosts_limits_and_redacted_auth() {
    let client = client();
    let platform = riot_api::platform::summoner_v4::SummonerByPuuid::new(
        PlatformRoute::Euw1,
        "public-puuid".to_owned(),
    );
    let platform_plan = client.plan(&platform).expect("platform plan");
    assert!(
        platform_plan
            .safe_url()
            .starts_with("https://euw1.api.riotgames.com/")
    );
    assert_eq!(platform_plan.rate_limit_rule_count(), 2);

    let regional = riot_api::regional::account_v1::AccountByRiotId::new(
        RegionalRoute::Europe,
        "Example Name".to_owned(),
        "TAG".to_owned(),
    );
    let regional_plan = client.plan(&regional).expect("regional plan");
    assert!(
        regional_plan
            .safe_url()
            .starts_with("https://europe.api.riotgames.com/")
    );
    assert_eq!(regional_plan.rate_limit_rule_count(), 2);

    let token_header = HeaderName::from_static("x-riot-token");
    for plan in [&platform_plan, &regional_plan] {
        assert_eq!(plan.public_headers().get(&token_header), None);
        assert!(plan.capabilities().authentication());
        let safe_surfaces = format!(
            "{}\n{:?}\n{:?}",
            plan.safe_url(),
            plan.public_headers(),
            plan
        );
        assert!(!safe_surfaces.contains(API_KEY_SENTINEL));
    }
}

#[test]
fn optional_defaults_pagination_binding_and_explicit_description_are_predictable() {
    let client = client();
    let default_endpoint = riot_api::regional::match_v5::MatchIdsByPuuid::new(
        RegionalRoute::Europe,
        "public-puuid".to_owned(),
    );
    let default_plan = client.plan(&default_endpoint).expect("default query plan");
    assert!(default_plan.safe_url().contains("start=%3Credacted%3E"));
    assert!(default_plan.safe_url().contains("count=%3Credacted%3E"));
    assert!(!default_plan.safe_url().contains("startTime="));
    assert!(!default_plan.safe_url().contains("endTime="));
    assert!(!default_plan.safe_url().contains("queue="));
    assert!(!default_plan.safe_url().contains("type="));

    let endpoint = riot_api::regional::match_v5::MatchIdsByPuuid::new(
        RegionalRoute::Europe,
        "public-puuid".to_owned(),
    )
    .set_match_type(MatchType::Ranked)
    .set_start(40)
    .set_count(NonZeroU32::new(10).expect("positive page size"));
    let description = endpoint.description();
    assert_eq!(description.method(), Method::GET);
    assert!(
        description
            .source_path()
            .ends_with("regional::match_v5::match_ids_by_puuid")
    );
    assert!(description.capabilities().authentication());
    assert!(description.capabilities().rate_limits());
    assert!(description.capabilities().pagination());

    let plan = client.plan(&endpoint).expect("configured query plan");
    assert!(plan.safe_url().contains("type=ranked"));
    assert!(plan.safe_url().contains("start=%3Credacted%3E"));
    assert!(plan.safe_url().contains("count=%3Credacted%3E"));
    assert_eq!(plan.endpoint_id(), description.id());
    assert_eq!(plan.rate_limit_rule_count(), 2);
    assert!(plan.capabilities().pagination());

    let _request: PaginatedRequest<_> = client.request(endpoint).paginate(PageLimit::Take(2));
    assert!(!format!("{description:?}\n{plan:?}").contains(API_KEY_SENTINEL));
}

#[test]
fn data_dragon_is_an_independent_importable_contract() {
    let client = DDragonApi::builder()
        .set_locale(DDragonLocale::FrFr)
        .build()
        .expect("Data Dragon client");
    let endpoint = d_dragon_api::version::data::Champions::new("15.1.1".to_owned());
    let plan = client.plan(&endpoint).expect("Data Dragon plan");
    assert_eq!(plan.method(), Method::GET);
    assert_eq!(plan.rate_limit_rule_count(), 0);
    assert!(
        plan.safe_url()
            .contains("/cdn/15.1.1/data/fr_FR/champion.json")
    );
    assert!(!plan.capabilities().authentication());
}
