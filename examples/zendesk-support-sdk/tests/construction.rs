//! Account destinations are checked before credentials are attached.
use concord_sdk_zendesk_support::{Account, SendPolicy, Zendesk, ZendeskConfig};

#[test]
fn account_construction_checks_production_and_fixture_bases() {
    for subdomain in ["", "a.evil", "bad/name", "-acme", "acme-", "ac_me"] {
        assert!(Account::new(subdomain).is_err());
    }
    for base in [
        "https://evil.example",
        "https://zendesk.com.evil.example",
        "http://127.0.0.1",
    ] {
        assert!(Account::for_fixture("acme", base).is_err());
    }
    let config = ZendeskConfig {
        subdomain: Account::new("acme").unwrap(),
        authorization: concord::reqwest::header::HeaderValue::from_static("Bearer ACCOUNT_CANARY"),
        policy: SendPolicy::default(),
    };
    let api = Zendesk::new(concord::reqwest::Client::new(), config);
    assert_eq!(
        api.ticket(1)
            .show()
            .prepare()
            .unwrap()
            .request()
            .url()
            .as_str(),
        "https://acme.zendesk.com/api/v2/tickets/1"
    );
    assert!(!format!("{api:?}").contains("ACCOUNT_CANARY"));
    let fixture = Zendesk::connect_account(
        concord::reqwest::Client::new(),
        Account::for_fixture("acme", "http://zendesk.test:1234/prefix").unwrap(),
        "fresh",
    )
    .unwrap();
    assert_eq!(
        fixture.base_url().as_str(),
        "http://acme.zendesk.test:1234/prefix"
    );
}
