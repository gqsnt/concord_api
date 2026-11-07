use concord_sdk_digitalocean::{DigitalOcean, PageSize};
use concord_testserver::{Recorded, Reply, TestServer};
use std::num::NonZeroUsize;

pub const TOKEN: &str = "SYNTHETIC_SECRET";

pub fn client(server: &TestServer) -> DigitalOcean {
    DigitalOcean::for_fixture(TOKEN, &server.url()).unwrap()
}

pub fn size() -> PageSize {
    PageSize::new(1).unwrap()
}
pub fn cap(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

pub fn fixture(body: &str, request: &Recorded) -> String {
    body.replace("__ORIGIN__", &format!("http://{}", request.host))
}

pub fn workflow_reply(request: &Recorded) -> Reply {
    let body = match request.target.as_str() {
        "/v2/droplets?per_page=1" => include_str!("../../fixtures/droplets-first.json"),
        "/v2/droplets?page=2&per_page=1" => include_str!("../../fixtures/droplets-last.json"),
        "/v2/droplets/3164444" => include_str!("../../fixtures/droplet.json"),
        "/v2/droplets/3164444/actions?per_page=1" => {
            include_str!("../../fixtures/actions-first.json")
        }
        "/v2/droplets/3164444/actions?page=2&per_page=1" => {
            include_str!("../../fixtures/actions-last.json")
        }
        "/v2/droplets/3164444/actions/7515" => include_str!("../../fixtures/action.json"),
        _ => panic!("unexpected fixture request target"),
    };
    Reply::json(200, fixture(body, request))
        .header("ratelimit-limit", "5000")
        .header("ratelimit-remaining", "4999")
        .header("ratelimit-reset", "1402425459")
}

pub fn assert_wire(requests: &[Recorded]) {
    for request in requests {
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.header_all("authorization"),
            ["Bearer SYNTHETIC_SECRET"]
        );
        assert_eq!(request.header("accept"), Some("application/json"));
        assert!(request.header("content-type").is_none());
        assert!(request.header("referer").is_none());
        assert_eq!(request.body_len, 0);
    }
}
