concord::api! {
    pub client Api {
        base "http://x.test";
        GET list(query limit: u32 = 20, query cursor?: String);
    }
}
fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    let _ = api.list().set_limit(5u8);
    let _ = api.list().maybe_cursor(Some("x"));
    let _ = api.list().maybe_cursor("x");
}
