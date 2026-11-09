//! Summary and Prepared Debug guarantees apply with tracing off as well as on.

concord::api! {
    client QueryProbe {
        base "http://query.invalid";
        GET read(query(..) pairs: Vec<(String, String)>);
    }
}

#[test]
fn dynamic_queries_are_always_withheld_in_debug_and_summary() {
    let prepared = QueryProbe::new(concord::reqwest::Client::new())
        .read(vec![(
            "CANARYSUMMARYKEY".into(),
            "CANARYSUMMARYVALUE".into(),
        )])
        .prepare()
        .unwrap();
    let debug = format!("{prepared:?}");
    let summary = prepared.summary().to_string();
    assert!(
        debug.contains("http://query.invalid/?<withheld>"),
        "{debug}"
    );
    assert_eq!(
        summary,
        "QueryProbe.read: GET http://query.invalid/?<withheld>"
    );
    assert_eq!(format!("{:?}", prepared.summary()), summary);
    for rendered in [debug, summary] {
        for canary in ["CANARYSUMMARYKEY", "CANARYSUMMARYVALUE"] {
            assert!(!rendered.contains(canary), "{rendered}");
        }
    }
    assert_eq!(
        prepared.request().url().query(),
        Some("CANARYSUMMARYKEY=CANARYSUMMARYVALUE")
    );
}
