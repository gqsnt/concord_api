// `#[deprecated]` on an endpoint warns where the endpoint is called, not inside the expansion.
#![deny(deprecated)]
concord::api! {
    pub client Api {
        base "http://x.test";
        #[deprecated(note = "use `items`")]
        GET list();
        GET items();
    }
}
fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    let _ = api.items();
    let _ = api.list();
}
