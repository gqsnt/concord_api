//! The capsule's loopback-only boundary applies to every construction route.
use concord_sdk_usgs_earthquake_catalog::{FixtureOrigin, Usgs, UsgsConfig};

#[test]
fn a_direct_config_requires_a_loopback_origin() {
    for input in [
        "https://earthquake.usgs.gov",
        "http://evil.example",
        "http://192.0.2.1",
        "http://user:secret@localhost",
        "http://127.0.0.1:0",
        "http://localhost?secret",
        "file:///tmp/secret",
    ] {
        assert!(FixtureOrigin::parse(input).is_err(), "{input}");
    }
    for input in [
        "http://127.0.0.1:8080/prefix",
        "http://[::1]:8080/prefix",
        "http://api.localhost:8080/prefix",
    ] {
        let api = Usgs::new(
            concord_sdk_usgs_earthquake_catalog::concord::reqwest::Client::new(),
            UsgsConfig {
                origin: FixtureOrigin::parse(input).unwrap(),
            },
        );
        assert_eq!(api.base_url().path(), "/prefix");
    }
}
