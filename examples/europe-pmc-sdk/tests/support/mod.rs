#![allow(dead_code)]
use concord_sdk_europe_pmc::{EuropePmc, PageSize, Query, http_client};
use concord_testserver::{Recorded, Reply, TestServer};

pub const FIRST_JSON: &str = include_str!("../../fixtures/search-first.json");
pub const LAST_JSON: &str = include_str!("../../fixtures/search-last.json");
pub const FIRST_XML: &str = include_str!("../../fixtures/search-first.xml");
pub const LAST_XML: &str = include_str!("../../fixtures/search-last.xml");
pub const EMPTY_JSON: &str = include_str!("../../fixtures/empty.json");
pub const EMPTY_XML: &str = include_str!("../../fixtures/empty.xml");
pub const SECRET: &str = "synthetic-SECRET-DO-NOT-LOG";

pub fn api(server: &TestServer) -> EuropePmc {
    EuropePmc::new(http_client().unwrap())
        .with_base_url(format!("{}/rest", server.url()))
        .unwrap()
}

pub fn search(api: &EuropePmc) -> concord_sdk_europe_pmc::EuropePmcSearch {
    api.search(Query::new("malaria").unwrap(), PageSize::new(2).unwrap())
}

pub async fn fixed(status: u16, media: &'static str, body: &'static str) -> TestServer {
    TestServer::start(move |_: &Recorded| -> Reply { Reply::text(status, media, body) }).await
}

pub fn redacted(value: &(impl std::fmt::Debug + std::fmt::Display)) {
    let printed = format!("{value:?} {value}");
    assert!(!printed.contains(SECRET), "secret leaked: {printed}");
}
