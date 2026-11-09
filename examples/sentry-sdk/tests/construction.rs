//! Checked deployment selection precedes credentials and caller admission state.
use concord_sdk_sentry::{Deployment, SendPolicy, Sentry, SentryConfig, http_client};

#[test]
fn deployment_checks_apply_to_direct_config_construction() {
    for input in [
        "http://evil.example",
        "https://user:secret@sentry.example",
        "https://sentry.example?secret",
        "https://sentry.example#secret",
        "http://127.0.0.1:0",
        "file:///tmp/secret",
    ] {
        assert!(Deployment::parse(input).is_err(), "{input}");
    }
    let api = Sentry::new(
        http_client().unwrap(),
        SentryConfig {
            deployment: Deployment::parse("https://sentry.example/proxy").unwrap(),
            authorization: concord_auth::bearer("secret").unwrap(),
            policy: SendPolicy::default(),
        },
    );
    assert_eq!(
        api.organization("acme")
            .issues()
            .prepare()
            .unwrap()
            .request()
            .url()
            .as_str(),
        "https://sentry.example/proxy/api/0/organizations/acme/issues/?limit=100"
    );
    assert!(!format!("{api:?}").contains("secret"));
    assert!(Deployment::parse("http://127.0.0.1:8080/proxy").is_ok());
    assert!(Deployment::parse("http://[::1]:8080").is_ok());
}
