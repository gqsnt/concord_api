concord::api! {
    pub client Api {
        base "http://x.test";
        GET item(id: u64, limit: u32 = 1) { path "items/{id}"; query { "limit" = limit; } }
    }
}
fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    let _ = api.item(1).set_id(2);
    let _ = api.item(1).maybe_limit(None);
}
