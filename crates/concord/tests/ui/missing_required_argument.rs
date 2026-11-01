concord::api! {
    pub client Api(tenant: String) {
        base "http://x.test";
        query { "tenant" = tenant; }
        GET item(id: u64) { path "items/{id}"; }
    }
}
fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    let _ = api.item(1);
}
