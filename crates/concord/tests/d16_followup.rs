//! Hand-calculated multi-owner composition cases and compiler-negative controls.
#![allow(missing_docs)]

concord::api! {
    client Topology(locale: String = "root") {
        base "http://topology.invalid/base";
        query { "k" = "root"; "root" = locale; }
        headers { "x-k" = "root"; }
        scope east(locale: String = "east") {
            query { append "k" = "east"; "east" = locale; }
            headers { append "x-k" = "east"; }
            scope inner(locale: String = "inner") {
                query { override "k" = "inner"; "inner" = locale; }
                headers { override "x-k" = "inner"; }
                GET leaf() {
                    query { append "k" = "leaf"; "client_ref" = client.locale; "east_ref" = east.locale; "inner_ref" = inner.locale; }
                    headers { append "x-k" = "leaf"; }
                }
                GET replaced() {
                    query { remove "k"; "k" = "new"; append "k" = "last"; }
                    headers { remove "x-k"; "x-k" = "new"; append "x-k" = "last"; }
                }
            }
        }
        scope west(locale: String = "west") {
            query { override "k" = "west"; "west" = locale; }
            headers { override "x-k" = "west"; }
            GET leaf() { query { "client_ref" = client.locale; "west_ref" = west.locale; } }
        }
    }
}

fn check<T>(request: concord::Prepared<T>, query: &str, headers: &[&str]) {
    assert_eq!(
        request.request().url().as_str(),
        format!("http://topology.invalid/base?{query}")
    );
    let values: Vec<_> = request
        .request()
        .headers()
        .get_all("x-k")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(values, headers);
}

#[test]
fn nested_owners_and_sibling_defaults() {
    let api = Topology::new(concord::reqwest::Client::new());
    check(
        api.east().inner().leaf().prepare().unwrap(),
        "k=inner&k=leaf&root=root&east=east&inner=inner&client_ref=root&east_ref=east&inner_ref=inner",
        &["inner", "leaf"],
    );
    check(
        api.east().inner().replaced().prepare().unwrap(),
        "k=new&k=last&root=root&east=east&inner=inner",
        &["new", "last"],
    );
    check(
        api.west().leaf().prepare().unwrap(),
        "k=west&root=root&west=west&client_ref=root&west_ref=west",
        &["west"],
    );
    let api = api.set_locale("R");
    check(
        api.east()
            .set_locale("E")
            .inner()
            .set_locale("I")
            .leaf()
            .prepare()
            .unwrap(),
        "k=inner&k=leaf&root=R&east=E&inner=I&client_ref=R&east_ref=E&inner_ref=I",
        &["inner", "leaf"],
    );
    check(
        api.west().leaf().prepare().unwrap(),
        "k=west&root=R&west=west&client_ref=R&west_ref=west",
        &["west"],
    );
}

#[test]
fn compiler_rejections() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui_d16/*.rs");
}
