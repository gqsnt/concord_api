//! A value declared before it is placed: the error is reported, the declaration is rejected, but
//! the client is still generated, so code using it elsewhere reports nothing else.

concord::api! {
    pub client Api(token: String, unused: String) {
        base "http://late.invalid";
        headers { "authorization" = "Bearer {token}"; }
        GET item(id: u32) { path "items/{id}"; }
    }
}

mod elsewhere {
    pub fn request() -> super::ApiItem {
        let config = super::ApiConfig { token: "t".into(), unused: "u".into() };
        super::Api::new(concord::reqwest::Client::new(), config).item(2)
    }
}

fn main() {
    let _ = elsewhere::request().prepare();
}
