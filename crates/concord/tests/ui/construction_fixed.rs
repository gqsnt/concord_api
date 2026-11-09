// C1/C2: a required client or scope value without `mut` has no setter, and a client whose base
// is a declared value has no `with_base_url`. Each forbidden move fails at the caller's line.
#[derive(Clone)]
pub struct Instance(concord::BaseUrl);

impl AsRef<concord::BaseUrl> for Instance {
    fn as_ref(&self) -> &concord::BaseUrl {
        &self.0
    }
}

concord::api! {
    pub client Org(
        instance: Instance,
        #[sensitive] header("authorization") mut token: String,
        version: u32,
    ) {
        base instance;
        path "v{version}";
        scope shop(shop: String, #[sensitive] header("x-shop-token") mut shop_token: String) {
            subdomain "{shop}";
            GET info();
        }
    }
}

fn main() {
    let instance = Instance(concord::BaseUrl::parse("https://org.example.test").unwrap());
    let org = Org::new(
        concord::reqwest::Client::new(),
        OrgConfig { instance, token: "t".into(), version: 60 },
    );
    let _ = org.clone().with_base_url("https://evil.example");
    let _ = org.clone().set_version(40);
    let _ = org.shop("a", "token-a").set_shop("b");
    let _ = org.set_token("rotated").shop("a", "x").set_shop_token("y");
}
