//! Wire-level protocol: request shapes against the documented examples, every documented error
//! class, malformed / truncated / oversized / wrong-media bodies, and redaction of the API key.
//! All offline, against `concord-example-testserver`.
#![allow(missing_docs)]

mod support;

use std::future::Future;
use std::num::NonZeroU32;
use std::pin::Pin;
use std::time::Duration;

use concord::ErrorKind;
use concord::bytes::Bytes;
use concord::reqwest::StatusCode;
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use concord_sdk_rest_countries::{
    CodeKind, CountryCode, CountryPage, CredentialError, Error, ErrorBodyError, Fields, Filters,
    InputError, Premium, Property, ProviderClass, RESPONSE_BODY_LIMIT, RestCountries,
    RestCountriesConfig, bearer,
};

use support::{KEY, assert_get, client, fixture_messages, http, json};

type Call = Pin<Box<dyn Future<Output = Result<CountryPage, Error>> + Send>>;
/// A documented target, its decoded query pairs, and the call that should produce it.
type Case = (&'static str, Vec<(&'static str, &'static str)>, Call);
/// A scripted reply factory.
type MakeReply = Box<dyn Fn() -> Reply + Send + Sync>;

fn concord_error(error: &Error) -> &concord::Error {
    match error {
        Error::Concord(error) => error,
        other => panic!("expected a Concord error, got {other:?}"),
    }
}

fn provider_error(error: &Error) -> &concord_sdk_rest_countries::ProviderError {
    error
        .provider()
        .unwrap_or_else(|| panic!("expected a provider error, got {error:?}"))
}

fn fields(properties: &[Property]) -> Fields {
    Fields::new(properties.iter().cloned()).unwrap()
}

/// The documented example requests (Countries API page, each endpoint's "Path examples" and
/// "Example"), sent through the generated client. Expected targets are the documented ones;
/// the only difference is that a comma inside `response_fields` travels as `%2C` (Concord's
/// query encoding), so the decoded pairs are also compared with the documented text.
#[tokio::test]
async fn documented_request_examples_reach_the_wire_as_documented() {
    let server = TestServer::start(|_| json(200, fixture!("synthetic-no-match.json"))).await;
    let api = client(&server);
    let europe = Filters::new().with(Property::REGION, "Europe").unwrap();
    let europe_eu = europe.clone().with(Property::MEMBERSHIPS_EU, "1").unwrap();
    let cases: Vec<Case> = vec![
        (
            "/countries/v5?response_fields=names.common%2Ccodes.alpha_2%2Cflag.emoji&limit=3",
            vec![
                ("response_fields", "names.common,codes.alpha_2,flag.emoji"),
                ("limit", "3"),
            ],
            Box::pin(
                api.countries()
                    .set_response_fields(fields(&[
                        Property::NAMES_COMMON,
                        Property::CODES_ALPHA_2,
                        Property::FLAG_EMOJI,
                    ]))
                    .set_limit(NonZeroU32::new(3).unwrap())
                    .send(),
            ),
        ),
        (
            "/countries/v5?limit=50&offset=100",
            vec![("limit", "50"), ("offset", "100")],
            Box::pin(
                api.countries()
                    .set_limit(NonZeroU32::new(50).unwrap())
                    .set_offset(100u64)
                    .send(),
            ),
        ),
        (
            "/countries/v5?response_fields=names&response_fields_omit=names.translations",
            vec![
                ("response_fields", "names"),
                ("response_fields_omit", "names.translations"),
            ],
            Box::pin(
                api.countries()
                    .set_response_fields(fields(&[Property::NAMES]))
                    .set_response_fields_omit(fields(&[Property::NAMES_TRANSLATIONS]))
                    .send(),
            ),
        ),
        (
            "/countries/v5?region=Europe&memberships.eu=1",
            vec![("region", "Europe"), ("memberships.eu", "1")],
            Box::pin(api.countries().set_filters(europe_eu).send()),
        ),
        (
            "/countries/v5?q=germ&region=Europe",
            vec![("q", "germ"), ("region", "Europe")],
            Box::pin(api.countries().set_q("germ").set_filters(europe).send()),
        ),
        (
            "/countries/v5/name?q=can",
            vec![("q", "can")],
            Box::pin(api.search_names("can").send()),
        ),
        (
            "/countries/v5/name?q=land&limit=10&offset=10",
            vec![("q", "land"), ("limit", "10"), ("offset", "10")],
            Box::pin(
                api.search_names("land")
                    .set_limit(NonZeroU32::new(10).unwrap())
                    .set_offset(10u64)
                    .send(),
            ),
        ),
        (
            "/countries/v5/code?q=usa",
            vec![("q", "usa")],
            Box::pin(api.search_codes("usa").send()),
        ),
        (
            "/countries/v5/codes.alpha_2/CA",
            vec![],
            Box::pin(api.country(&CountryCode::parse("ca").unwrap()).send()),
        ),
        (
            "/countries/v5/codes.alpha_3/CAN",
            vec![],
            Box::pin(api.country(&CountryCode::parse("CAN").unwrap()).send()),
        ),
        (
            "/countries/v5/subregion/Northern%20Europe",
            vec![],
            Box::pin(
                api.property(Property::SUBREGION)
                    .read("Northern Europe")
                    .send(),
            ),
        ),
        (
            "/countries/v5/borders/CAN",
            vec![],
            Box::pin(api.property(Property::BORDERS).read("CAN").send()),
        ),
        (
            "/countries/v5/region/Europe?q=germ",
            vec![("q", "germ")],
            Box::pin(
                api.property(Property::REGION)
                    .read("Europe")
                    .set_q("germ")
                    .send(),
            ),
        ),
        (
            "/countries/v5/names.common?q=ger",
            vec![("q", "ger")],
            Box::pin(api.property(Property::NAMES_COMMON).search("ger").send()),
        ),
        (
            "/countries/v5/memberships.eu/true",
            vec![],
            Box::pin(api.property(Property::MEMBERSHIPS_EU).read("true").send()),
        ),
    ];
    let expected: Vec<(&str, Vec<(&str, &str)>)> =
        cases.iter().map(|(t, q, _)| (*t, q.clone())).collect();
    for (_, _, call) in cases {
        let page = call.await.unwrap();
        assert!(page.countries.is_empty());
    }
    let requests = server.requests();
    assert_eq!(requests.len(), expected.len());
    for (request, (target, pairs)) in requests.iter().zip(expected) {
        assert_get(request, target);
        let decoded: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        assert_eq!(request.query_pairs(), decoded, "{target}");
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn documented_response_examples_decode_into_typed_pages() {
    let cases: [(&[u8], u64, &[&str]); 5] = [
        (
            fixture!("doc-list-fields-limit3.json"),
            249,
            &["Afghanistan", "Albania", "Algeria"],
        ),
        (fixture!("doc-search-germ-europe.json"), 1, &["Germany"]),
        (
            fixture!("doc-name-can.json"),
            3,
            &["Canada", "Cape Verde", "Cameroon"],
        ),
        (fixture!("doc-capitals-tokyo.json"), 1, &["Japan"]),
        (
            fixture!("doc-names-common-ger.json"),
            4,
            &["Germany", "Niger", "Nigeria", "Algeria"],
        ),
    ];
    for (body, total, names) in cases {
        let server = TestServer::start(move |_| json(200, body)).await;
        let page = client(&server).countries().send().await.unwrap();
        let got: Vec<&str> = page
            .countries
            .iter()
            .filter_map(|c| c.common_name())
            .collect();
        assert_eq!(got, names);
        assert_eq!(page.meta.total, total);
        assert_eq!(page.meta.count, names.len() as u64);
        assert!(page.demo.is_none());
        // Only the selected properties exist; nothing else is invented.
        for country in &page.countries {
            assert!(country.other.is_empty());
            assert!(country.population.is_none() && country.leaders.is_none());
        }
        assert_eq!(server.shutdown().await.open_after, 0);
    }

    // The list example (limit 3 of 249): typed fields and the documented continuation.
    let server = TestServer::start(|_| json(200, fixture!("doc-list-fields-limit3.json"))).await;
    let page = client(&server).countries().send().await.unwrap();
    assert_eq!(page.countries[2].alpha_2(), Some("DZ"));
    let flag = page.countries[0].flag.as_ref().unwrap();
    assert_eq!(flag.emoji.as_deref(), Some("🇦🇫"));
    assert!(page.meta.more);
    assert_eq!(page.next_offset(), Ok(Some(3)));
    let germ = TestServer::start(|_| json(200, fixture!("doc-search-germ-europe.json"))).await;
    let germany = client(&germ).countries().send().await.unwrap();
    assert_eq!(germany.countries[0].region.as_deref(), Some("Europe"));
    assert_eq!(germany.next_offset(), Ok(None));
    assert_eq!(server.shutdown().await.open_after, 0);
    assert_eq!(germ.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn path_and_query_values_cannot_inject_segments_or_parameters() {
    let server = TestServer::start(|_| json(200, fixture!("synthetic-no-match.json"))).await;
    let api = client(&server);
    let value = "Europe/../codes.alpha_2/CA?x=1#f";
    api.property(Property::REGION)
        .read(value)
        .set_q("a&limit=500 b+c")
        .send()
        .await
        .unwrap();
    let request = server.single();
    assert_eq!(request.method, "GET");
    let segments: Vec<&str> = request.path().split('/').collect();
    assert_eq!(segments.len(), 5, "{}", request.target); // "", countries, v5, region, value
    assert_eq!(&segments[..4], ["", "countries", "v5", "region"]);
    assert!(!segments[4].contains(['/', '?', '#']));
    assert!(!request.target.contains('#'));
    assert_eq!(
        request.query_pairs(),
        [("q".to_owned(), "a&limit=500 b+c".to_owned())]
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[test]
fn inputs_with_provider_rules_are_refused_before_any_request() {
    for bad in [
        "",
        "Region",
        "names..common",
        ".region",
        "region.",
        "region=x",
        "a b",
        "née",
    ] {
        assert_eq!(Property::new(bad), Err(InputError::Property), "{bad:?}");
    }
    for good in [
        "region",
        "names.common",
        "date.fiscal_year_start.corporate.basis",
        "g20",
    ] {
        assert_eq!(Property::new(good).unwrap().as_str(), good);
    }
    for documented in Property::DOCUMENTED {
        assert_eq!(&Property::new(documented.as_str()).unwrap(), documented);
    }
    assert_eq!(Fields::new([]), Err(InputError::EmptyFields));
    for reserved in [
        "q",
        "limit",
        "offset",
        "response_fields",
        "response_fields_omit",
        "pretty",
    ] {
        let property = Property::new(reserved).unwrap();
        assert_eq!(
            Filters::new().with(property, "x"),
            Err(InputError::ReservedFilter),
            "{reserved}"
        );
    }
    let replaced = Filters::new()
        .with(Property::REGION, "Asia")
        .unwrap()
        .with(Property::LANDLOCKED, "1")
        .unwrap()
        .with(Property::REGION, "Europe")
        .unwrap();
    assert_eq!(
        replaced.pairs(),
        [
            (Property::REGION, "Europe".to_owned()),
            (Property::LANDLOCKED, "1".to_owned())
        ]
    );
    for bad in ["", "C", "CANA", "C1", "ÇA", "c a"] {
        assert_eq!(
            CountryCode::parse(bad),
            Err(InputError::CountryCode),
            "{bad:?}"
        );
    }
    let alpha_2 = CountryCode::parse("de").unwrap();
    assert_eq!((alpha_2.kind(), alpha_2.as_str()), (CodeKind::Alpha2, "DE"));
    assert_eq!(alpha_2.property(), Property::CODES_ALPHA_2);
    let alpha_3 = CountryCode::parse("Deu").unwrap();
    assert_eq!(
        (alpha_3.kind(), alpha_3.as_str()),
        (CodeKind::Alpha3, "DEU")
    );
    assert_eq!(alpha_3.property(), Property::CODES_ALPHA_3);

    for bad in ["", "rc key", "rc_key\n", "clé", "rc\tkey"] {
        assert_eq!(bearer(bad), Err(CredentialError::Token), "{bad:?}");
    }
    let header = bearer(KEY).unwrap();
    assert!(header.is_sensitive());
    assert_eq!(header.to_str().unwrap(), format!("Bearer {KEY}"));
}

#[test]
fn header_names_compose_on_every_endpoint() {
    RestCountries::validate_header_names().unwrap();
}

/// Every documented status (Countries API "Errors"), each with a JSON `errors` document: the
/// status, its documented class and the document's messages, after exactly one request.
#[tokio::test]
async fn every_documented_error_class_is_a_provider_error_after_one_request() {
    let cases: [(u16, &[u8], ProviderClass); 9] = [
        (
            400,
            fixture!("doc-error-400-limit.json"),
            ProviderClass::BadRequest,
        ),
        (
            401,
            fixture!("synthetic-error-401.json"),
            ProviderClass::Unauthorized,
        ),
        (
            403,
            fixture!("synthetic-error-403-frozen.json"),
            ProviderClass::Forbidden,
        ),
        (
            403,
            fixture!("doc-error-403-premium.json"),
            ProviderClass::Forbidden,
        ),
        (
            404,
            fixture!("synthetic-error-404.json"),
            ProviderClass::NotFound,
        ),
        (
            405,
            fixture!("synthetic-error-405.json"),
            ProviderClass::MethodNotAllowed,
        ),
        (
            410,
            fixture!("synthetic-error-410.json"),
            ProviderClass::Gone,
        ),
        (
            429,
            fixture!("synthetic-error-429.json"),
            ProviderClass::RateLimited,
        ),
        (
            500,
            fixture!("synthetic-error-500.json"),
            ProviderClass::Server,
        ),
    ];
    for (status, body, class) in cases {
        let server =
            TestServer::start(move |_| json(status, body).header("retry-after", "7")).await;
        let error = client(&server)
            .countries()
            .set_limit(NonZeroU32::new(101).unwrap())
            .send()
            .await
            .unwrap_err();
        assert_eq!(error.status(), Some(StatusCode::from_u16(status).unwrap()));
        let provider = provider_error(&error);
        assert_eq!(provider.class(), class, "{status}");
        let messages: Vec<&str> = provider.messages().collect();
        assert_eq!(messages, fixture_messages(body), "{status}");
        assert_eq!(provider.meta.headers()["retry-after"], "7");
        assert_eq!(error.operation().unwrap().name(), "RestCountries.countries");
        // One request: no retry, no credential refresh, no follow-up page.
        assert_get(&server.single(), "/countries/v5?limit=101");
        assert_eq!(server.shutdown().await.open_after, 0);
    }
    assert_eq!(
        ProviderClass::of(StatusCode::BAD_GATEWAY),
        ProviderClass::Server
    );
    assert_eq!(
        ProviderClass::of(StatusCode::IM_A_TEAPOT),
        ProviderClass::Undocumented
    );
}

#[tokio::test]
async fn unreadable_error_documents_keep_the_status() {
    let oversized = {
        let mut body = br#"{"errors":[{"message":""#.to_vec();
        body.extend(std::iter::repeat_n(b'x', 80 * 1024));
        body.extend_from_slice(br#""}]}"#);
        body
    };
    let replies: Vec<(u16, MakeReply, ErrorKind)> = vec![
        (
            502,
            Box::new(|| Reply::text(502, "text/html", fixture!("synthetic-proxy-502.html"))),
            ErrorKind::Metadata,
        ),
        (404, Box::new(|| Reply::new(404)), ErrorKind::Metadata),
        (
            500,
            Box::new(|| json(500, fixture!("synthetic-error-wrong-shape.json"))),
            ErrorKind::Decode,
        ),
        (
            400,
            Box::new(|| json(400, fixture!("synthetic-malformed.json"))),
            ErrorKind::Decode,
        ),
        (
            400,
            Box::new(move || json(400, &oversized)),
            ErrorKind::Limit,
        ),
        (
            401,
            Box::new(|| {
                Reply::new(401)
                    .header("content-type", "application/json")
                    .with_body(Body::FailAfter(vec![Bytes::from_static(
                        br#"{"errors":[{"mess"#,
                    )]))
            }),
            ErrorKind::Transport,
        ),
    ];
    for (status, reply, kind) in replies {
        let server = TestServer::start(move |_| reply()).await;
        let error = client(&server).search_codes("de").send().await.unwrap_err();
        assert_eq!(error.status(), Some(StatusCode::from_u16(status).unwrap()));
        let provider = provider_error(&error);
        let body_error = provider.document.as_ref().unwrap_err();
        let source = match (kind, body_error) {
            (ErrorKind::Metadata, ErrorBodyError::Metadata(e))
            | (ErrorKind::Decode, ErrorBodyError::Decode(e))
            | (ErrorKind::Limit | ErrorKind::Transport, ErrorBodyError::Read(e)) => e,
            other => panic!("{status}: unexpected {other:?}"),
        };
        assert_eq!(source.kind(), kind, "{status}");
        assert_eq!(provider.messages().count(), 0);
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn malformed_or_misshapen_success_bodies_are_decode_errors_with_the_head() {
    let bodies: Vec<&[u8]> = vec![
        fixture!("synthetic-malformed.json"),
        fixture!("synthetic-truncated.json"),
        fixture!("synthetic-errors-on-200.json"),
        fixture!("synthetic-missing-meta.json"),
        fixture!("synthetic-wrong-types.json"),
        b"",
        b"null",
        b"[]",
        br#"{"data":{"objects":{},"meta":{"total":0,"count":0,"limit":25,"offset":0,"more":false}}}"#,
    ];
    for body in bodies {
        let server = TestServer::start(move |_| json(200, body)).await;
        let error = client(&server).countries().send().await.unwrap_err();
        let source = concord_error(&error);
        assert_eq!(
            source.kind(),
            ErrorKind::Decode,
            "{}",
            String::from_utf8_lossy(body)
        );
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert_eq!(
            source.head().unwrap().headers()["x-fixture"],
            "rest-countries-offline"
        );
        assert_eq!(server.requests().len(), 1);
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn wrong_media_types_are_refused_before_the_body_is_read() {
    let replies: [fn() -> Reply; 4] = [
        || Reply::text(200, "text/html", fixture!("synthetic-proxy-502.html")),
        || Reply::text(200, "text/plain", fixture!("synthetic-no-match.json")),
        || Reply::new(200).body(fixture!("synthetic-no-match.json")),
        || {
            Reply::new(200)
                .header("content-type", "application/json")
                .header("content-type", "application/json")
                .body(fixture!("synthetic-no-match.json"))
        },
    ];
    for reply in replies {
        let server = TestServer::start(move |_| reply()).await;
        let error = client(&server).countries().send().await.unwrap_err();
        let source = concord_error(&error);
        assert_eq!(source.kind(), ErrorKind::Metadata);
        assert_eq!(error.status(), Some(StatusCode::OK));
        assert_eq!(server.shutdown().await.open_after, 0);
    }
}

#[tokio::test]
async fn a_body_cut_off_mid_transfer_is_a_transport_error_after_the_head() {
    let server = TestServer::start(|_| {
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::FailAfter(vec![Bytes::from_static(fixture!(
                "synthetic-truncated.json"
            ))]))
    })
    .await;
    let error = client(&server).countries().send().await.unwrap_err();
    let source = concord_error(&error);
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert_eq!(source.delivery(), concord::Delivery::Answered);
    assert_eq!(error.status(), Some(StatusCode::OK));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn an_oversized_success_body_stops_at_the_limit_without_decoding() {
    const TOTAL: usize = 4 * RESPONSE_BODY_LIMIT;
    const CHUNK: usize = 256 * 1024;
    let server = TestServer::start(|_| {
        let mut sent = 0usize;
        let generator = Box::new(move || {
            if sent >= TOTAL {
                return None;
            }
            let chunk = if sent == 0 {
                let mut first = br#"{"data":{"objects":["#.to_vec();
                first.resize(CHUNK, b' ');
                first
            } else {
                vec![b' '; CHUNK]
            };
            sent += chunk.len();
            Some(Bytes::from(chunk))
        });
        Reply::new(200)
            .header("content-type", "application/json")
            .with_body(Body::Generate(generator, None))
    })
    .await;
    let error = client(&server).countries().send().await.unwrap_err();
    let source = concord_error(&error);
    assert_eq!(source.kind(), ErrorKind::Limit);
    assert_eq!(error.status(), Some(StatusCode::OK));
    assert!(
        server
            .wait_until(Duration::from_secs(5), |s| s.active_bodies() == 0)
            .await
    );
    assert!(server.body_bytes_sent() < TOTAL as u64);
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn the_demo_notice_is_surfaced_and_unknown_fields_are_kept() {
    let server = TestServer::start(|_| json(200, fixture!("synthetic-demo-sample.json"))).await;
    let page = client(&server).countries().send().await.unwrap();
    assert!(page.demo.is_some());
    assert_eq!(page.countries[0].alpha_2(), Some("CA"));
    assert_eq!(server.shutdown().await.open_after, 0);

    let server = TestServer::start(|_| json(200, fixture!("synthetic-germany-deu.json"))).await;
    let germany = client(&server)
        .country(&CountryCode::parse("DEU").unwrap())
        .send()
        .await
        .unwrap();
    assert_eq!(
        germany.meta.request_id.as_deref(),
        Some("synthetic-request-1")
    );
    let germany = germany.single().unwrap().unwrap();
    assert_eq!(germany.other["a_future_v5_field"]["added"], "within v5");
    assert_eq!(germany.other["cars"]["driving_side"], "right");
    assert!(matches!(germany.leaders, Some(Premium::Withheld { .. })));
    assert_eq!(server.shutdown().await.open_after, 0);
}

/// The key never appears in Concord's or the SDK's `Debug`/`Display`, also when a provider
/// message echoes it (a synthetic 401 document written for this test); it stays readable
/// through the deliberate accessors only.
#[tokio::test]
async fn the_api_key_is_redacted_from_debug_and_display() {
    let echo = format!(r#"{{"errors":[{{"message":"Synthetic: key {KEY} is revoked."}}]}}"#);
    let server = TestServer::start(move |_| json(401, echo.as_bytes())).await;
    let api = client(&server);
    let shown = format!("{api:?}");
    assert!(!shown.contains(KEY), "{shown}");
    assert!(shown.contains("redacted"), "{shown}");

    let prepared = api.countries().prepare().unwrap();
    assert!(prepared.request().headers()["authorization"].is_sensitive());
    assert!(!format!("{prepared:?}").contains(KEY));
    let request = api.country(&CountryCode::parse("CA").unwrap());
    assert!(!format!("{request:?}").contains(KEY));

    let error = api.countries().send().await.unwrap_err();
    for text in [format!("{error:?}"), error.to_string()] {
        assert!(!text.contains(KEY), "{text}");
        assert!(!text.contains("revoked"), "{text}");
    }
    let provider = provider_error(&error);
    assert_eq!(provider.class(), ProviderClass::Unauthorized);
    assert!(provider.messages().any(|m| m.contains(KEY)));
    assert_eq!(server.requests().len(), 1);

    let config = RestCountriesConfig {
        authorization: bearer(KEY).unwrap(),
    };
    let refused = RestCountries::new(http(), config)
        .with_base_url("http://127.0.0.1:9")
        .unwrap()
        .countries()
        .send()
        .await
        .unwrap_err();
    let source = concord_error(&refused);
    assert_eq!(source.kind(), ErrorKind::Transport);
    assert!(!format!("{refused:?} {refused}").contains(KEY));
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn responses_on_unexpected_routes_are_not_reinterpreted() {
    // A redirect is not followed by the test client and is a provider status, not a page.
    let server = TestServer::start(|r: &Recorded| {
        assert_eq!(r.path(), "/countries/v5/code");
        Reply::new(302).header("location", "https://elsewhere.example.invalid/")
    })
    .await;
    let error = client(&server).search_codes("ca").send().await.unwrap_err();
    assert_eq!(error.status(), Some(StatusCode::FOUND));
    assert_eq!(provider_error(&error).class(), ProviderClass::Undocumented);
    assert_eq!(server.requests().len(), 1);
    assert_eq!(server.shutdown().await.open_after, 0);
}
