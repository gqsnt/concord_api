// A `page next url` path is Rust field access on the declared output: an unknown field, `?` on a
// field that is not an `Option`, and a last field that is not URL text fail at the path.
use concord::Json;

#[derive(serde::Deserialize)]
pub struct Pages {
    pub next: Option<String>,
    pub count: u64,
}

#[derive(serde::Deserialize)]
pub struct Page {
    pub items: Vec<u64>,
    pub links: Option<Pages>,
    pub pages: Pages,
}

concord::api! {
    pub client Paths {
        base "http://x.test";
        format Json;
        GET unknown() -> Page { page next url links?.nxt; }
        GET not_option() -> Page { page next url pages?.next; }
        GET not_text() -> Page { page next url links?.count; }
    }
}

fn main() {}
