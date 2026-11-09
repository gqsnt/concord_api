#![allow(dead_code)]

use concord_sdk_usgs_earthquake_catalog::{Magnitude, PageSize, Usgs, Window, concord::reqwest};
use concord_testserver::{Body, Reply, TestServer};
use std::time::Duration;

pub const WINDOW: &str = include_str!("../../fixtures/window.geojson");
pub const PAGE1: &str = include_str!("../../fixtures/page-1.geojson");
pub const PAGE2: &str = include_str!("../../fixtures/page-2.geojson");
pub const EVENT: &str = include_str!("../../fixtures/event.geojson");
pub const CSV: &str = include_str!("../../fixtures/window.csv");
pub const CSV1: &str = include_str!("../../fixtures/page-1.csv");
pub const CSV2: &str = include_str!("../../fixtures/page-2.csv");
pub const HEADER: &str = include_str!("../../fixtures/header-only.csv");
pub const ERROR_TEXT: &str = include_str!("../../fixtures/error.txt");

pub fn client(server: &TestServer) -> Usgs {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    Usgs::for_fixture(http, &server.url()).unwrap()
}

pub fn filter(size: u16) -> Window {
    Window::new("2014-01-01T00:00:00Z", "2014-01-02T00:00:00Z")
        .unwrap()
        .min_magnitude(Magnitude::new(4.0).unwrap())
        .with_page_size(PageSize::new(size).unwrap())
}

pub fn csv(text: &str) -> Reply {
    Reply::text(200, "text/csv; charset=utf-8", text)
}

pub fn fragmented(text: &str) -> Reply {
    use concord_sdk_usgs_earthquake_catalog::concord::bytes::Bytes;
    csv("").with_body(Body::Chunks(
        text.as_bytes()
            .chunks(1)
            .map(|chunk| (Duration::ZERO, Bytes::copy_from_slice(chunk)))
            .collect(),
    ))
}
