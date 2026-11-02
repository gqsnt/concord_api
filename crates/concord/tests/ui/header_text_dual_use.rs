// A value placed both as a whole header value and as text must be text itself (or `#[sensitive]`):
// a type's own `ToHeaderValue` can flag its header value sensitive, which a path, query value or
// template cannot see (Codex review of 66a4478, P1).
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
        GET item() { path "item"; query { "s" = client.sig; } }
    }
}

fn main() {}
