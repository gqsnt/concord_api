use concord_example_testserver::{Reply, TestServer};
use concord_sdk_intercom::{Intercom, concord::reqwest};

pub const FIRST: &str = include_str!("../../fixtures/contact-first.json");
pub const CREATED: &str = include_str!("../../fixtures/contact-created.json");
pub const LIST_FIRST: &str = include_str!("../../fixtures/list-first.json");
pub const LIST_LAST: &str = include_str!("../../fixtures/list-last.json");
pub const RATE_ERROR: &str = include_str!("../../fixtures/rate-error.json");

pub fn api(server: &TestServer) -> Intercom {
    // Wire accounting is independent of environment proxy settings and reqwest defaults.
    let http = reqwest::Client::builder()
        .no_proxy()
        .retry(reqwest::retry::never())
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(2))
        .build()
        .unwrap();
    Intercom::with_token(http, "SYNTHETIC_SECRET")
        .unwrap()
        .with_base_url(server.url())
        .unwrap()
}

pub fn rate_error() -> Reply {
    Reply::json(429, RATE_ERROR)
}
