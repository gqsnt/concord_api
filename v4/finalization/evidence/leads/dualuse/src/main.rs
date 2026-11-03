// Lead: the dual-use rule (a value placed as a header AND as text must be text or #[sensitive])
// is enforced by the bound on the generated header call. Does it still hold when every header
// contribution of that value is removed for an endpoint?
use concord::ToHeaderValue;
use concord::reqwest::header::HeaderValue;

#[derive(Clone)]
pub struct Signature(String);

impl ToHeaderValue for Signature {
    fn to_header_value(&self) -> Option<HeaderValue> {
        let mut value = HeaderValue::from_str(&self.0).ok()?;
        value.set_sensitive(true);
        Some(value)
    }
}

impl std::fmt::Display for Signature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

concord::api! {
    pub client Api(sig: Signature) {
        base "http://x.test";
        headers { "x-signature" = sig; }
        GET item() { path "item"; query { "s" = client.sig; } headers { remove "x-signature"; } }
    }
}

fn main() {
    let api = Api::new(reqwest::Client::new(), ApiConfig { sig: Signature("NATIVE-SECRET".into()) });
    let prepared = api.item().prepare().expect("prepare");
    println!("{prepared:?}");
}
