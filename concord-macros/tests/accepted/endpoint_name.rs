// `Endpoint::NAME` is the stable name of an endpoint, the one its errors and `TryInfo` carry.
// An author may give an endpoint type an inherent `NAME` of its own: the trait's constant,
// and the name that generated code reports, stay the same.
concord::api! {
    pub client Api {
        base "https://names.test/";
        GET get(n: String) { query { "n" = n.parse::<u32>()?; } }
        scope nested {
            GET get(n: String) { query { "n" = n.parse::<u32>()?; } }
        }
    }
}

impl api::Get {
    pub const NAME: &'static str = "the author's own";
}
impl api::nested::Get {
    pub const NAME: &'static str = "the author's own";
}

fn name<E: api::Endpoint>() -> &'static str {
    E::NAME
}

fn main() {
    assert_eq!(name::<api::Get>(), "api.get");
    assert_eq!(<api::nested::Get as api::Endpoint>::NAME, "api.nested.get");
    assert_eq!(api::Get::NAME, "the author's own");
    let error = Api::new().get("x").request().unwrap_err();
    assert_eq!(error.endpoint(), "api.get");
    let error = Api::new().nested().get("x").request().unwrap_err();
    assert_eq!(error.endpoint(), "api.nested.get");
}
