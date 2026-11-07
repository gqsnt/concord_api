//! Request parity and strict provider-code validation, using local fixtures only.
use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_fred::{
    AggregationMethod, Error, Fred, FredConfig, Frequency, ObservationOptions, Units, http_client,
};

// Independently enumerated expectations from series_observations.html (2026-11-07).
const UNITS: &[(&str, Units)] = &[
    ("lin", Units::Levels),
    ("chg", Units::Change),
    ("ch1", Units::ChangeFromYearAgo),
    ("pch", Units::PercentChange),
    ("pc1", Units::PercentChangeFromYearAgo),
    ("pca", Units::CompoundedAnnualRateOfChange),
    ("cch", Units::ContinuouslyCompoundedRateOfChange),
    ("cca", Units::ContinuouslyCompoundedAnnualRateOfChange),
    ("log", Units::NaturalLog),
];
const FREQUENCIES: &[(&str, Frequency)] = &[
    ("d", Frequency::Daily),
    ("w", Frequency::Weekly),
    ("bw", Frequency::Biweekly),
    ("m", Frequency::Monthly),
    ("q", Frequency::Quarterly),
    ("sa", Frequency::Semiannual),
    ("a", Frequency::Annual),
    ("wef", Frequency::WeeklyEndingFriday),
    ("weth", Frequency::WeeklyEndingThursday),
    ("wew", Frequency::WeeklyEndingWednesday),
    ("wetu", Frequency::WeeklyEndingTuesday),
    ("wem", Frequency::WeeklyEndingMonday),
    ("wesu", Frequency::WeeklyEndingSunday),
    ("wesa", Frequency::WeeklyEndingSaturday),
    ("bwew", Frequency::BiweeklyEndingWednesday),
    ("bwem", Frequency::BiweeklyEndingMonday),
];
const METHODS: &[(&str, AggregationMethod)] = &[
    ("avg", AggregationMethod::Average),
    ("sum", AggregationMethod::Sum),
    ("eop", AggregationMethod::EndOfPeriod),
];

#[test]
fn documented_codes_are_exact_and_unknown_codes_are_rejected() {
    macro_rules! validate {
        ($ty:ty, $cases:expr, $parameter:literal, $invalid:expr) => {
            for &(wire, value) in $cases {
                assert_eq!(<$ty>::new(wire).unwrap(), value);
                assert_eq!(wire.parse::<$ty>().unwrap(), value);
                assert_eq!(value.as_str(), wire);
                assert_eq!(value.to_string(), wire);
            }
            for invalid in $invalid {
                let error = <$ty>::new(invalid).unwrap_err();
                assert!(matches!(
                    error,
                    Error::InvalidParameter {
                        parameter: $parameter
                    }
                ));
                assert!(invalid.parse::<$ty>().is_err());
                assert_eq!(error.to_string(), concat!("invalid FRED ", $parameter));
                assert!(!format!("{error:?}").contains("synthetic-secret"));
            }
        };
    }
    validate!(
        Units,
        UNITS,
        "units",
        [
            "",
            "LIN",
            "lin ",
            "percent",
            "q",
            ".",
            "0",
            "synthetic-secret"
        ]
    );
    validate!(
        Frequency,
        FREQUENCIES,
        "frequency",
        [
            "",
            "D",
            "monthly",
            "we",
            "bwef",
            " wef",
            "wef ",
            "pch",
            ".",
            "synthetic-secret"
        ]
    );
    validate!(
        AggregationMethod,
        METHODS,
        "aggregation_method",
        ["", "AVG", "average", "sum ", "q", ".", "synthetic-secret"]
    );
}

fn configured<E: ObservationOptions>(
    endpoint: E,
    units: Units,
    frequency: Frequency,
    method: AggregationMethod,
) -> E {
    endpoint
        .set_units(units)
        .set_frequency(frequency)
        .set_aggregation_method(method)
}

fn fixture(r: &Recorded) -> Reply {
    match r.query_all("file_type").first().map(String::as_str) {
        Some("json") => Reply::json(200, include_bytes!("../fixtures/observations-0.json")),
        Some("csv") => Reply::text(
            200,
            "application/zip",
            include_bytes!("../fixtures/observations.zip"),
        ),
        None => Reply::text(
            200,
            "text/xml",
            include_bytes!("../fixtures/observations-0.xml"),
        ),
        other => panic!("unexpected format {other:?}"),
    }
}

#[tokio::test]
async fn every_documented_control_reaches_all_three_forms_without_value_coercion() {
    let server = TestServer::start(fixture).await;
    let api = Fred::new(
        http_client().unwrap(),
        FredConfig {
            api_key: "local-key".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let series = api.series("SYN_A");
    // Cover every code in every format without an unnecessary Cartesian product.
    for (i, &(frequency_wire, frequency)) in FREQUENCIES.iter().enumerate() {
        let (units_wire, units) = UNITS[i % UNITS.len()];
        let (method_wire, method) = METHODS[i % METHODS.len()];
        let xml = configured(series.observations(), units, frequency, method)
            .send()
            .await
            .unwrap();
        let json = configured(series.observations_json(), units, frequency, method)
            .send()
            .await
            .unwrap();
        for page in [xml, json] {
            assert_eq!(page.observations[0].value, "2.5000");
            assert_eq!(page.observations[1].value, ".");
        }
        let export = configured(series.export_csv(), units, frequency, method)
            .send()
            .await
            .unwrap();
        assert_eq!(export.csv(), include_bytes!("../fixtures/observations.csv"));
        // ZIP decoding, not plain-CSV decoding, still preserves every original byte.
        assert_eq!(
            export.readme(),
            Some(&include_bytes!("../fixtures/README.txt")[..])
        );

        let requests = server.requests();
        assert_eq!(requests.len(), (i + 1) * 3);
        for (r, format, accept) in [
            (&requests[i * 3], None, "text/xml"),
            (&requests[i * 3 + 1], Some("json"), "application/json"),
            (&requests[i * 3 + 2], Some("csv"), "application/zip"),
        ] {
            assert_eq!(r.method, "GET");
            assert_eq!(r.path(), "/fred/series/observations");
            assert_eq!(r.body_len, 0);
            assert_eq!(r.header("accept"), Some(accept));
            assert_eq!(r.query_pairs().len(), if format.is_some() { 8 } else { 7 });
            for (key, value) in [
                ("api_key", "local-key"),
                ("series_id", "SYN_A"),
                ("limit", "100000"),
                ("offset", "0"),
                ("units", units_wire),
                ("frequency", frequency_wire),
                ("aggregation_method", method_wire),
            ] {
                assert_eq!(r.query_all(key), [value], "{key} in {format:?}");
            }
            assert_eq!(
                r.query_all("file_type"),
                format.into_iter().collect::<Vec<_>>()
            );
        }
    }
    server.shutdown().await;
}

fn cleared<E: ObservationOptions>(endpoint: E) -> E {
    configured(
        endpoint,
        Units::NaturalLog,
        Frequency::Annual,
        AggregationMethod::Sum,
    )
    .maybe_units(None)
    .maybe_frequency(None)
    .maybe_aggregation_method(None)
}

#[tokio::test]
async fn controls_can_be_cleared_to_preserve_provider_defaults_in_all_forms() {
    let server = TestServer::start(fixture).await;
    let api = Fred::new(
        http_client().unwrap(),
        FredConfig {
            api_key: "local-key".into(),
        },
    )
    .with_base_url(server.url())
    .unwrap();
    let series = api.series("SYN_A");
    cleared(series.observations()).send().await.unwrap();
    cleared(series.observations_json()).send().await.unwrap();
    cleared(series.export_csv()).send().await.unwrap();
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for r in requests {
        for key in ["units", "frequency", "aggregation_method"] {
            assert!(r.query_all(key).is_empty(), "{key} should be omitted");
        }
    }
    server.shutdown().await;
}
