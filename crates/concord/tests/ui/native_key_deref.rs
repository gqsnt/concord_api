// A key path must name a `HeaderName` itself (H08): a type that dereferences to one is not
// accepted (no deref coercion to the native name), whatever its `Deref` returns.
use concord::reqwest::header::HeaderName;

static PUBLIC: HeaderName = HeaderName::from_static("x-public");

pub struct Wrapped;

impl std::ops::Deref for Wrapped {
    type Target = HeaderName;
    fn deref(&self) -> &HeaderName {
        &PUBLIC
    }
}

pub static WRAPPED: Wrapped = Wrapped;

concord::api! {
    pub client Api(header(WRAPPED) token: String) {
        base "http://x.test";
        GET get();
    }
}

fn main() {}
