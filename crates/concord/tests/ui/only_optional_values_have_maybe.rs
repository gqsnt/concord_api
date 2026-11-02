// Every value has `set_x`; only optional values (and an optional body) have `maybe_x`.
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item(id: u64, limit: u32 = 1) { path "items/{id}"; query { "limit" = limit; } }
    }
}
fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    let _ = api.item(1).set_id(2).set_limit(3);
    let _ = api.item(1).maybe_id(None);
    let _ = api.item(1).maybe_limit(None);
}
