//! Fresh-consumer applications built from the public README and generated rustdoc.
//! Every provider response is an authored sample served on loopback.

use std::{
    num::{NonZeroU32, NonZeroUsize},
    time::Duration,
};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_rest_countries::concord::{ErrorKind, reqwest};
use concord_sdk_rest_countries::concord_pagination::{PagesError, Resume};
use concord_sdk_rest_countries::{
    CountryCode, Error, Fields, Filters, Premium, Property, ProviderClass, RestCountries,
    RestCountriesConfig, bearer, pages,
};
use serde_json::{Value, json};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const KEY: &str = "rc_fixture_sdk50_secret";
const NOTICE: &str = "leaders is unavailable on this fixture's free plan.";
const SELECTED: &str = "names.common,codes.alpha_2,leaders,borders";

fn client(server: &TestServer) -> Result<RestCountries, Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()?;
    Ok(RestCountries::new(
        http,
        RestCountriesConfig {
            authorization: bearer(KEY)?,
        },
    )
    .with_base_url(server.url())?)
}

fn sample(objects: Value, total: u64, limit: u64, offset: u64, request_id: &str) -> Reply {
    let count = objects
        .as_array()
        .expect("fixture objects are an array")
        .len() as u64;
    Reply::json(
        200,
        json!({
            "data": {
                "objects": objects,
                "meta": {
                    "total": total, "count": count, "limit": limit, "offset": offset,
                    "more": offset + count < total, "request_id": request_id
                },
                "_demo": {"message": "Authored local sample; not the complete provider dataset."}
            }
        })
        .to_string(),
    )
    .header("x-request-id", request_id)
}

fn assert_wire(server: &TestServer, request: &Recorded, path: &str, query: &[(&str, &str)]) {
    assert_eq!(request.method, "GET");
    assert_eq!(request.path(), path);
    let mut actual = request.query_pairs();
    let mut expected: Vec<_> = query
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(
        request.header_all("authorization"),
        [format!("Bearer {KEY}")]
    );
    assert_eq!(request.header("accept"), Some("application/json"));
    assert_eq!(request.host, server.addr().to_string());
    assert!(request.body.is_empty());
    assert_eq!(request.body_len, 0);
    assert!(!request.target.contains(KEY));
    assert!(request.query_all("api-key").is_empty());
}

#[tokio::test]
async fn search_names_then_fetch_chosen_country() -> TestResult {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        let q = request.query_all("q");
        match (request.path(), q.first().map(String::as_str)) {
            ("/countries/v5/name", Some("germ")) => sample(
                json!([{"names": {"common": "Germany"}, "codes": {"alpha_3": "DEU"}}]),
                1,
                25,
                0,
                "search-germany",
            ),
            ("/countries/v5/codes.alpha_3/DEU", None) => sample(
                json!([{
                    "names": {"common": "Germany", "official": "Federal Republic of Germany"},
                    "codes": {"alpha_2": "DE", "alpha_3": "DEU"},
                    "region": "Europe",
                    "borders": ["AUT", "BEL", "CHE", "CZE", "DNK", "FRA", "LUX", "NLD", "POL"]
                }]),
                1,
                25,
                0,
                "lookup-germany",
            ),
            ("/countries/v5/name", Some("Atlantis")) => sample(json!([]), 0, 25, 0, "no-match"),
            ("/countries/v5/name", Some("")) => Reply::json(
                400,
                json!({"errors": [{"message": format!("Empty q; fixture echo {KEY}")}]})
                    .to_string(),
            )
            .header("x-request-id", "empty-query"),
            ("/countries/v5/name", Some("canad")) => Reply::json(
                401,
                r#"{"errors":[{"message":"Fixture key was revoked."}]}"#,
            )
            .header("x-request-id", "revoked-key"),
            ("/countries/v5/name", Some("fran")) => {
                Reply::json(200, r#"{"data":{"objects":["#).header("x-request-id", "broken-json")
            }
            _ => Reply::json(
                500,
                r#"{"errors":[{"message":"Unexpected fixture request."}]}"#,
            ),
        }
    })
    .await;
    let api = client(&server)?;
    let search = api.search_names("germ").set_response_fields(Fields::new([
        Property::NAMES_COMMON,
        Property::CODES_ALPHA_3,
    ])?);
    assert!(!format!("{api:?}").contains(KEY));
    assert!(!format!("{search:?}").contains(KEY));

    let candidates = search.send().await?;
    assert_eq!(server.requests().len(), 1);
    assert_eq!(candidates.meta.total, 1);
    assert_eq!(candidates.meta.count, 1);
    assert_eq!(candidates.meta.limit, 25);
    assert_eq!(candidates.meta.offset, 0);
    assert!(!candidates.meta.more);
    assert_eq!(
        candidates.meta.request_id.as_deref(),
        Some("search-germany")
    );
    assert!(candidates.demo.is_some());
    assert_eq!(candidates.next_offset()?, None);
    let chosen = candidates
        .countries
        .iter()
        .find(|country| country.common_name() == Some("Germany"))
        .expect("the user can choose Germany");
    assert!(chosen.borders.is_none());
    assert!(chosen.alpha_2().is_none());
    eprintln!(
        "FIRST_WORKING_CALL: docs-only search_names(germ), one loopback GET, decoded Germany"
    );

    let code = CountryCode::parse(chosen.alpha_3().expect("selected alpha-3 code"))?;
    assert_eq!(code, CountryCode::parse("deu")?);
    let lookup = api.country(&code).send().await?;
    assert_eq!(lookup.meta.request_id.as_deref(), Some("lookup-germany"));
    assert_eq!(lookup.next_offset()?, None);
    let germany = lookup.single()?.expect("the chosen country still exists");
    assert_eq!(germany.common_name(), Some("Germany"));
    assert_eq!(germany.alpha_2(), Some("DE"));
    assert_eq!(germany.borders.as_ref().map(Vec::len), Some(9));
    assert!(
        germany
            .borders
            .as_ref()
            .unwrap()
            .iter()
            .any(|code| code == "FRA")
    );

    let empty = api.search_names("Atlantis").send().await?;
    assert!(empty.countries.is_empty());
    assert_eq!((empty.meta.total, empty.meta.count), (0, 0));
    assert_eq!(empty.next_offset()?, None);
    assert!(empty.single()?.is_none());
    let before_empty_walk = server.requests().len();
    let no_matches = pages(api.search_names("Atlantis"))
        .max_pages(NonZeroUsize::new(2).unwrap())
        .items()
        .collect_complete()
        .await?;
    assert!(no_matches.is_empty());
    assert_eq!(server.requests().len(), before_empty_walk + 1);

    let bad_query = api.search_names("").send().await.unwrap_err();
    assert_eq!(bad_query.status().unwrap().as_u16(), 400);
    assert!(bad_query.operation().is_some());
    let provider = bad_query.provider().expect("400 is a provider outcome");
    assert_eq!(provider.class(), ProviderClass::BadRequest);
    assert_eq!(
        provider.messages().collect::<Vec<_>>(),
        [format!("Empty q; fixture echo {KEY}")]
    );
    assert_eq!(provider.meta.headers()["x-request-id"], "empty-query");
    assert!(!format!("{bad_query:?}").contains(KEY));
    assert!(!bad_query.to_string().contains(KEY));

    let before_unauthorized = server.requests().len();
    let unauthorized = api.search_names("canad").send().await.unwrap_err();
    assert_eq!(unauthorized.status().unwrap().as_u16(), 401);
    assert_eq!(
        unauthorized.provider().unwrap().class(),
        ProviderClass::Unauthorized
    );
    assert_eq!(server.requests().len(), before_unauthorized + 1);

    let broken = api.search_names("fran").send().await.unwrap_err();
    assert_eq!(broken.status().unwrap().as_u16(), 200);
    let Error::Concord(decode) = broken else {
        panic!("malformed success must be a decode error")
    };
    assert_eq!(decode.kind(), ErrorKind::Decode);
    let head = decode.head().expect("decode failure retains the head");
    assert_eq!(head.status().as_u16(), 200);
    assert_eq!(head.headers()["x-request-id"], "broken-json");

    let before_validation = server.requests().len();
    assert!(CountryCode::parse("DEUT").is_err());
    let invalid_key = bearer("not a valid key").unwrap_err();
    assert!(!format!("{invalid_key:?} {invalid_key}").contains("not a valid key"));
    assert_eq!(server.requests().len(), before_validation);

    let requests = server.requests();
    assert_eq!(
        requests.len(),
        7,
        "no implicit pagination, retry or auth calls"
    );
    assert_wire(
        &server,
        &requests[0],
        "/countries/v5/name",
        &[
            ("q", "germ"),
            ("response_fields", "names.common,codes.alpha_3"),
        ],
    );
    assert_wire(
        &server,
        &requests[1],
        "/countries/v5/codes.alpha_3/DEU",
        &[],
    );
    for (request, query) in requests[2..]
        .iter()
        .zip(["Atlantis", "Atlantis", "", "canad", "fran"])
    {
        assert_wire(&server, request, "/countries/v5/name", &[("q", query)]);
    }
    server.shutdown().await;
    Ok(())
}

#[tokio::test]
async fn filter_select_fields_and_keep_partial_results() -> TestResult {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        if request.path() != "/countries/v5" {
            return Reply::json(
                500,
                r#"{"errors":[{"message":"Unexpected fixture path."}]}"#,
            );
        }
        if !request.query_all("leaders").is_empty() {
            return Reply::json(
                403,
                r#"{"errors":[{"message":"Premium filters require a paid plan."}]}"#,
            )
            .header("x-request-id", "premium-filter");
        }
        if !request.query_all("population").is_empty() {
            return Reply::json(
                404,
                r#"{"errors":[{"message":"population is not searchable."}]}"#,
            )
            .header("x-request-id", "unsearchable-filter");
        }
        let offset = request
            .query_all("offset")
            .first()
            .map(|value| value.parse::<u64>().expect("numeric offset"))
            .unwrap_or(0);
        if request.query_all("q") == ["republic"] && offset == 1 {
            return Reply::json(
                429,
                r#"{"errors":[{"message":"Fixture account rate limit reached."}]}"#,
            )
            .header("retry-after", "10")
            .header("x-request-id", "second-page-limited");
        }
        match offset {
            0 => sample(
                json!([{
                    "names": {"common": "France"}, "codes": {"alpha_2": "FR"},
                    "leaders": [{"message": NOTICE}]
                }]),
                2,
                1,
                0,
                "eu-first",
            ),
            1 => sample(
                json!([{
                    "names": {"common": "Germany"}, "codes": {"alpha_2": "DE"},
                    "leaders": [{"message": NOTICE}]
                }]),
                2,
                1,
                1,
                "eu-last",
            ),
            _ => sample(json!([]), 2, 1, offset, "past-end"),
        }
    })
    .await;
    let api = client(&server)?;
    let filters = Filters::new()
        .with(Property::REGION, "Europe")?
        .with(Property::MEMBERSHIPS_EU, "1")?;
    let fields = Fields::new([
        Property::NAMES_COMMON,
        Property::CODES_ALPHA_2,
        Property::LEADERS,
        Property::BORDERS,
    ])?;
    let listing = api
        .countries()
        .set_filters(filters)
        .set_response_fields(fields)
        .set_limit(NonZeroU32::new(1).unwrap());

    let first = listing.clone().send().await?;
    assert_eq!(
        server.requests().len(),
        1,
        "send returns one page even with more=true"
    );
    assert_eq!(
        (
            first.meta.total,
            first.meta.count,
            first.meta.limit,
            first.meta.offset
        ),
        (2, 1, 1, 0)
    );
    assert!(first.meta.more);
    assert_eq!(first.meta.request_id.as_deref(), Some("eu-first"));
    assert!(first.demo.is_some());
    assert_eq!(first.next_offset()?, Some(1));
    let france = &first.countries[0];
    assert_eq!(france.common_name(), Some("France"));
    assert_eq!(france.alpha_2(), Some("FR"));
    assert!(france.alpha_3().is_none());
    assert!(
        france.region.is_none(),
        "a filter is not an implicitly selected field"
    );
    assert!(
        france.borders.is_none(),
        "a selected but omitted field is ordinary missing data"
    );
    assert!(matches!(&france.leaders, Some(Premium::Withheld { message }) if message == NOTICE));
    assert_eq!(
        server.requests().len(),
        1,
        "unavailable fields cause no pagination or auth work"
    );

    let walk = pages(listing.clone()).max_pages(NonZeroUsize::new(2).unwrap());
    assert_eq!(
        server.requests().len(),
        1,
        "constructing the walker sends nothing"
    );
    let all = walk.items().collect_complete().await?;
    assert_eq!(
        all.iter()
            .filter_map(|country| country.common_name())
            .collect::<Vec<_>>(),
        ["France", "Germany"]
    );
    assert_eq!(
        all.iter()
            .filter_map(|country| country.alpha_2())
            .collect::<Vec<_>>(),
        ["FR", "DE"]
    );
    assert!(all.iter().all(|country| country.borders.is_none()));
    assert!(all.iter().all(|country| matches!(&country.leaders, Some(Premium::Withheld { message }) if message == NOTICE)));
    assert_eq!(
        server.requests().len(),
        3,
        "the terminal page at the cap is complete"
    );

    let capped = pages(listing.clone())
        .max_pages(NonZeroUsize::new(1).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(capped.collected.len(), 1);
    assert_eq!(capped.collected[0].common_name(), Some("France"));
    assert!(matches!(&capped.error, PagesError::Bounded { .. }));
    assert_eq!(capped.error.resume(), Some(&Resume::At(1)));
    let progress = capped.error.progress();
    assert_eq!(
        (progress.admitted, progress.succeeded, progress.failed),
        (1, 1, 0)
    );
    assert_eq!(
        server.requests().len(),
        4,
        "a cap must not fetch the second page"
    );

    let partial = pages(listing.clone().set_q("republic"))
        .max_pages(NonZeroUsize::new(3).unwrap())
        .items()
        .collect_complete()
        .await
        .unwrap_err();
    assert_eq!(partial.collected.len(), 1);
    assert_eq!(partial.collected[0].common_name(), Some("France"));
    assert_eq!(partial.error.resume(), Some(&Resume::At(1)));
    let PagesError::Provider {
        error,
        failed,
        progress,
        ..
    } = partial.error
    else {
        panic!("the second-page status error must retain the useful prefix");
    };
    assert_eq!(failed, Resume::At(1));
    assert_eq!(
        (progress.admitted, progress.succeeded, progress.failed),
        (2, 1, 1)
    );
    assert_eq!(progress.in_flight, 0);
    assert_eq!(error.status().unwrap().as_u16(), 429);
    assert!(error.operation().is_some());
    let provider = error
        .provider()
        .expect("page failure retains the SDK provider error");
    assert_eq!(provider.class(), ProviderClass::RateLimited);
    assert_eq!(
        provider.messages().collect::<Vec<_>>(),
        ["Fixture account rate limit reached."]
    );
    assert_eq!(provider.meta.headers()["retry-after"], "10");
    assert_eq!(
        provider.meta.headers()["x-request-id"],
        "second-page-limited"
    );
    assert_eq!(
        server.requests().len(),
        6,
        "429 stops the walk without retry or refresh"
    );

    let past_end = listing.clone().set_offset(100).send().await?;
    assert!(past_end.countries.is_empty());
    assert_eq!(
        (
            past_end.meta.total,
            past_end.meta.count,
            past_end.meta.offset
        ),
        (2, 0, 100)
    );
    assert!(!past_end.meta.more);
    assert_eq!(past_end.next_offset()?, None);

    let forbidden = api
        .countries()
        .set_filters(Filters::new().with(Property::LEADERS, "president")?)
        .send()
        .await
        .unwrap_err();
    assert_eq!(forbidden.status().unwrap().as_u16(), 403);
    let provider = forbidden.provider().unwrap();
    assert_eq!(provider.class(), ProviderClass::Forbidden);
    assert_eq!(
        provider.messages().collect::<Vec<_>>(),
        ["Premium filters require a paid plan."]
    );
    assert_eq!(provider.meta.headers()["x-request-id"], "premium-filter");

    let unsearchable = api
        .countries()
        .set_filters(Filters::new().with(Property::POPULATION, "83000000")?)
        .send()
        .await
        .unwrap_err();
    assert_eq!(unsearchable.status().unwrap().as_u16(), 404);
    assert_eq!(
        unsearchable.provider().unwrap().class(),
        ProviderClass::NotFound
    );
    assert_eq!(
        unsearchable
            .provider()
            .unwrap()
            .messages()
            .collect::<Vec<_>>(),
        ["population is not searchable."]
    );

    let requests = server.requests();
    assert_eq!(
        requests.len(),
        9,
        "only the nine explicit fixture requests were sent"
    );
    let base = [
        ("region", "Europe"),
        ("memberships.eu", "1"),
        ("response_fields", SELECTED),
        ("limit", "1"),
    ];
    for index in [0, 1, 3] {
        assert_wire(&server, &requests[index], "/countries/v5", &base);
    }
    for (index, extra) in [
        (2, vec![("offset", "1")]),
        (4, vec![("q", "republic")]),
        (5, vec![("q", "republic"), ("offset", "1")]),
        (6, vec![("offset", "100")]),
    ] {
        let mut query = base.to_vec();
        query.extend(extra);
        assert_wire(&server, &requests[index], "/countries/v5", &query);
    }
    assert_wire(
        &server,
        &requests[7],
        "/countries/v5",
        &[("leaders", "president")],
    );
    assert_wire(
        &server,
        &requests[8],
        "/countries/v5",
        &[("population", "83000000")],
    );
    server.shutdown().await;
    Ok(())
}
