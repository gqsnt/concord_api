//! The two connected workflows of the slice, end to end against local fixtures, with the
//! required failure: a no-match result or an unavailable field stays an ordinary provider
//! outcome — no extra page request, no retry, no credential change.
#![allow(missing_docs)]

mod support;

use std::num::{NonZeroU32, NonZeroUsize};

use concord::reqwest::StatusCode;
use concord_example_testserver::{Recorded, TestServer};
use concord_sdk_rest_countries::{
    ContractError, CountryCode, Error, Fields, Filters, Premium, Property, ProviderClass, pages,
};

use support::{assert_get, client, fixture_messages, json};

fn names(countries: &[concord_sdk_rest_countries::Country]) -> Vec<&str> {
    countries.iter().filter_map(|c| c.common_name()).collect()
}

/// Workflow 1: search country names, choose one candidate, fetch it by its ISO code.
#[tokio::test]
async fn workflow_1_search_names_then_fetch_the_chosen_country_by_code() {
    let server = TestServer::start(|request: &Recorded| match request.path() {
        "/countries/v5/name" => json(200, fixture!("synthetic-name-germ.json")),
        "/countries/v5/codes.alpha_3/DEU" => json(200, fixture!("synthetic-germany-deu.json")),
        _ => json(404, fixture!("synthetic-error-404.json")),
    })
    .await;
    let api = client(&server);

    let candidates = api
        .search_names("germ")
        .set_response_fields(
            Fields::new([
                Property::NAMES_COMMON,
                Property::NAMES_OFFICIAL,
                Property::CODES_ALPHA_3,
            ])
            .unwrap(),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(candidates.next_offset(), Ok(None));
    let chosen = candidates
        .countries
        .iter()
        .find(|country| country.common_name() == Some("Germany"))
        .unwrap();
    let code = CountryCode::parse(chosen.alpha_3().unwrap()).unwrap();

    let germany = api
        .country(&code)
        .send()
        .await
        .unwrap()
        .single()
        .unwrap()
        .expect("the code matched");
    let names_ = germany.names.as_ref().unwrap();
    assert_eq!(
        names_.official.as_deref(),
        Some("Federal Republic of Germany")
    );
    assert_eq!(
        names_.native.as_ref().unwrap()["deu"].common.as_deref(),
        Some("Deutschland")
    );
    assert_eq!(germany.alpha_2(), Some("DE"));
    assert_eq!(germany.region.as_deref(), Some("Europe"));
    assert_eq!(
        germany.capitals.as_ref().unwrap()[0].name.as_deref(),
        Some("Berlin")
    );
    assert_eq!(germany.landlocked, Some(false));
    assert!(germany.borders.as_ref().unwrap().iter().any(|b| b == "FRA"));
    assert_eq!(
        germany.population.as_ref().and_then(|p| p.as_u64()),
        Some(83_000_000)
    );
    assert_eq!(germany.memberships.as_ref().unwrap().get("eu"), Some(&true));
    assert_eq!(germany.other["tlds"][0], ".de");

    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_get(
        &requests[0],
        "/countries/v5/name?q=germ&response_fields=names.common%2Cnames.official%2Ccodes.alpha_3",
    );
    assert_get(&requests[1], "/countries/v5/codes.alpha_3/DEU");
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Required failure, workflow 1: a name search and a code lookup without match are successful,
/// empty results after exactly one request each; the bounded walk ends after its first page.
#[tokio::test]
async fn workflow_1_no_match_is_an_empty_result_not_a_failure() {
    let server = TestServer::start(|_| json(200, fixture!("synthetic-no-match.json"))).await;
    let api = client(&server);

    let page = api.search_names("zzzz").send().await.unwrap();
    assert!(page.countries.is_empty());
    assert_eq!((page.meta.total, page.meta.more), (0, false));
    assert_eq!(page.next_offset(), Ok(None));
    assert_eq!(server.requests().len(), 1);

    let missing = api
        .country(&CountryCode::parse("ZZ").unwrap())
        .send()
        .await
        .unwrap()
        .single()
        .unwrap();
    assert!(missing.is_none());
    assert_eq!(server.requests().len(), 2);

    let walked = pages(api.search_names("zzzz"))
        .max_pages(NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(walked.is_empty());

    let requests = server.requests();
    assert_eq!(requests.len(), 3, "no page request after an empty result");
    assert_get(&requests[0], "/countries/v5/name?q=zzzz");
    assert_get(&requests[1], "/countries/v5/codes.alpha_2/ZZ");
    assert_get(&requests[2], "/countries/v5/name?q=zzzz");
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// A code lookup that matches several records is refused rather than resolved by picking one.
#[tokio::test]
async fn workflow_1_an_ambiguous_code_lookup_is_a_contract_error() {
    let server = TestServer::start(|_| json(200, fixture!("synthetic-not-unique.json"))).await;
    let page = client(&server)
        .country(&CountryCode::parse("ZZ").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(page.single(), Err(ContractError::NotUnique { total: 2 }));
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Workflow 2: filter by documented properties (region, EU membership), select the response
/// fields, and read the whole matched set page by page.
#[tokio::test]
async fn workflow_2_filter_by_property_and_select_response_fields() {
    let server =
        TestServer::start(
            |request: &Recorded| match request.query_all("offset").as_slice() {
                [] => json(200, fixture!("synthetic-europe-eu-page1.json")),
                [offset] if offset == "2" => json(200, fixture!("synthetic-europe-eu-page2.json")),
                _ => json(400, fixture!("doc-error-400-limit.json")),
            },
        )
        .await;
    let api = client(&server);
    let filters = Filters::new()
        .with(Property::REGION, "Europe")
        .unwrap()
        .with(Property::MEMBERSHIPS_EU, "1")
        .unwrap();
    let selected = Fields::new([
        Property::NAMES_COMMON,
        Property::CODES_ALPHA_2,
        Property::MEMBERSHIPS_EU,
    ])
    .unwrap();
    let template = api
        .countries()
        .set_filters(filters)
        .set_response_fields(selected)
        .set_limit(NonZeroU32::new(2).unwrap());

    // One page: the provider says more records exist and where they start.
    let first = template.clone().send().await.unwrap();
    assert_eq!(names(&first.countries), ["Austria", "Belgium"]);
    assert_eq!(first.next_offset(), Ok(Some(2)));
    for country in &first.countries {
        assert!(country.alpha_2().is_some());
        assert_eq!(country.memberships.as_ref().unwrap().get("eu"), Some(&true));
        // Not selected, so absent: never an error.
        assert!(country.region.is_none() && country.flag.is_none());
        assert!(country.other.is_empty());
    }

    // The whole set, bounded.
    let all = pages(template)
        .max_pages(NonZeroUsize::new(10).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert_eq!(names(&all), ["Austria", "Belgium", "Bulgaria"]);

    let base = "/countries/v5?region=Europe&memberships.eu=1\
                &response_fields=names.common%2Ccodes.alpha_2%2Cmemberships.eu&limit=2";
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert_get(&requests[0], base);
    assert_get(&requests[1], base);
    assert_get(&requests[2], &format!("{base}&offset=2"));
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Required failure, workflow 2: unavailable fields. A premium field selected on a free plan
/// arrives as the documented notice; a selected field a record lacks is absent. Both pages
/// succeed after one request.
#[tokio::test]
async fn workflow_2_unavailable_selected_fields_are_ordinary_values() {
    let server = TestServer::start(|request: &Recorded| {
        match request.query_all("response_fields").as_slice() {
            [fields] if fields == "names.common,leaders" => {
                json(200, fixture!("derived-leaders-free-plan.json"))
            }
            [fields] if fields == "names.common,parent.alpha_2" => {
                json(200, fixture!("synthetic-parent-absent.json"))
            }
            _ => json(400, fixture!("doc-error-400-limit.json")),
        }
    })
    .await;
    let api = client(&server);

    let france = api
        .countries()
        .set_q("france")
        .set_response_fields(Fields::new([Property::NAMES_COMMON, Property::LEADERS]).unwrap())
        .send()
        .await
        .unwrap();
    let leaders = france.countries[0].leaders.as_ref().unwrap();
    let Premium::Withheld { message } = leaders else {
        panic!("expected the free-plan notice, got {leaders:?}");
    };
    assert!(message.starts_with("data.leaders is only available on paid plans."));
    assert_eq!(france.next_offset(), Ok(None));

    let parent = Property::new("parent.alpha_2").unwrap();
    let europe = api
        .countries()
        .set_filters(Filters::new().with(Property::REGION, "Europe").unwrap())
        .set_response_fields(Fields::new([Property::NAMES_COMMON, parent]).unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(names(&europe.countries), ["Germany", "Gibraltar"]);
    assert!(europe.countries[0].other.get("parent").is_none());
    assert_eq!(europe.countries[1].other["parent"]["alpha_2"], "GB");

    assert_eq!(server.requests().len(), 2);
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// Required failure, workflow 2: filtering or reading by a property that is not searchable or
/// readable (`404`), or by a premium property on a free plan (`403`), is a provider error with
/// its document after one request: the walk stops on it, nothing is retried, and the same
/// credential is sent (no auth machinery).
#[tokio::test]
async fn workflow_2_unavailable_filter_properties_are_provider_errors() {
    let server = TestServer::start(|request: &Recorded| {
        if request.path() == "/countries/v5/flag.emoji/DE" {
            return json(404, fixture!("synthetic-error-404.json"));
        }
        if !request.query_all("population").is_empty() {
            return json(404, fixture!("synthetic-error-404.json"));
        }
        if !request.query_all("leaders").is_empty() {
            return json(403, fixture!("doc-error-403-premium.json"));
        }
        json(400, fixture!("doc-error-400-limit.json"))
    })
    .await;
    let api = client(&server);
    let population = Filters::new()
        .with(Property::POPULATION, "83000000")
        .unwrap();
    let leaders = Filters::new().with(Property::LEADERS, "macron").unwrap();

    let error = api
        .countries()
        .set_filters(population.clone())
        .send()
        .await
        .unwrap_err();
    let provider = error.provider().unwrap();
    assert_eq!(provider.class(), ProviderClass::NotFound);
    assert_eq!(
        provider.messages().collect::<Vec<_>>(),
        fixture_messages(fixture!("synthetic-error-404.json"))
    );

    let error = api
        .property(Property::FLAG_EMOJI)
        .read("DE")
        .send()
        .await
        .unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::NOT_FOUND));

    let error = api
        .countries()
        .set_filters(leaders)
        .send()
        .await
        .unwrap_err();
    let provider = error.provider().unwrap();
    assert_eq!(provider.class(), ProviderClass::Forbidden);
    assert_eq!(
        provider.messages().collect::<Vec<_>>(),
        fixture_messages(fixture!("doc-error-403-premium.json"))
    );

    // The bounded walk stops at the failed first page and hands back the provider error.
    let incomplete = pages(api.countries().set_filters(population))
        .max_pages(NonZeroUsize::new(5).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert!(incomplete.collected.is_empty());
    let Some(Error::Provider(provider)) = incomplete.error.provider_error() else {
        panic!("expected the provider error, got {:?}", incomplete.error);
    };
    assert_eq!(provider.class(), ProviderClass::NotFound);

    let requests = server.requests();
    assert_eq!(
        requests.len(),
        4,
        "one request per call, no retry or follow-up"
    );
    assert_get(&requests[0], "/countries/v5?population=83000000");
    assert_get(&requests[1], "/countries/v5/flag.emoji/DE");
    assert_get(&requests[2], "/countries/v5?leaders=macron");
    assert_get(&requests[3], "/countries/v5?population=83000000");
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// An offset past the end is answered with an empty `200` page (documented): it ends the walk
/// and is not mistaken for an inconsistent page.
#[tokio::test]
async fn workflow_2_an_offset_past_the_end_ends_the_walk() {
    let server =
        TestServer::start(|_| json(200, fixture!("derived-out-of-range-offset.json"))).await;
    let template = client(&server).countries().set_offset(300u64);
    let page = template.clone().send().await.unwrap();
    assert_eq!(page.next_offset(), Ok(None));
    assert_eq!(page.meta.request_id.as_deref(), Some("<request-id>"));
    // Every walk in these tests is capped: a defect ends it with an error instead of a hang.
    let walked = pages(template)
        .max_pages(NonZeroUsize::new(3).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap();
    assert!(walked.is_empty());
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_get(&requests[1], "/countries/v5?offset=300");
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// A page whose `meta` contradicts itself ends the walk with a contract error instead of
/// requesting a page the provider did not announce (or stopping silently).
#[tokio::test]
async fn workflow_2_contradictory_pages_end_the_walk_with_a_contract_error() {
    for (body, expected) in [
        (
            fixture!("synthetic-more-mismatch.json"),
            ContractError::MoreMismatch,
        ),
        (
            fixture!("synthetic-count-mismatch.json"),
            ContractError::CountMismatch,
        ),
    ] {
        let server = TestServer::start(move |_| json(200, body)).await;
        let incomplete = pages(client(&server).countries())
            .max_pages(NonZeroUsize::new(3).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap_err();
        let Some(Error::Contract(error)) = incomplete.error.provider_error() else {
            panic!("expected a contract error, got {:?}", incomplete.error);
        };
        assert_eq!(*error, expected);
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}
