// A removed control is not generated: calling it is rustc's "no method" error. The controls not
// listed stay public.
concord::api! {
    pub client Api(subdomain: String, #[sensitive] header("x-token") token: String) {
        base "http://x.test";
        controls { remove set_subdomain; remove with_base_url; }
        subdomain "{subdomain}";
        scope shop(shop: String) {
            controls { remove set_shop; }
            path "{shop}";
            GET get();
        }
    }
}
fn main() {
    let http = concord::reqwest::Client::new();
    let api = Api::new(http, ApiConfig { subdomain: "a".into(), token: "t".into() });
    let api = api.set_token("rotated");
    let _ = api.clone().set_subdomain("b");
    let _ = api.clone().with_base_url("http://other.test");
    let _ = api.shop("s").set_shop("other");
}
