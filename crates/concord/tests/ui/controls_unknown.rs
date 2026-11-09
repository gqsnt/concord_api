// `controls` names generated controls of the owner it is written on.
concord::api! {
    pub client Api(subdomain: String, token?: String) {
        base "http://x.test";
        controls { remove set_subdmain; pub(crate) send; }
        subdomain "{subdomain}";
        headers { "x-token" = token; }
        scope shop(shop: String) {
            controls { pub(crate) with_base_url; }
            path "{shop}";
            GET get();
        }
        scope admin {
            controls { remove set_shop; }
            GET ping();
        }
    }
}
fn main() {}
