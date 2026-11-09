//! `serde_plain::derive_display_from_serialize!` supplies the wire spelling of a placed value.
//! A type that does not serialize as a scalar panics in serde_plain; Concord does not catch it.

use concord::reqwest;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
enum Sort {
    LongRunning,
}
serde_plain::derive_display_from_serialize!(Sort);

#[derive(Serialize)]
#[serde(transparent)]
struct Values(Vec<String>);
serde_plain::derive_display_from_serialize!(Values);

concord::api! {
    client Scalars {
        base "http://127.0.0.1:9";
        GET sort(query sort: Sort);
        GET list(query values: Values);
    }
}

#[test]
fn a_placed_value_is_the_derived_text() {
    let client = Scalars::new(reqwest::Client::new());
    let prepared = client.sort(Sort::LongRunning).prepare().unwrap();
    assert_eq!(
        prepared.request().url().as_str(),
        "http://127.0.0.1:9/?sort=long-running"
    );
}

#[test]
#[should_panic]
fn a_non_scalar_panics_in_the_adapter() {
    let client = Scalars::new(reqwest::Client::new());
    let _ = client.list(Values(vec!["a".into()])).prepare();
}
