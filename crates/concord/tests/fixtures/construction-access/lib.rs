//! A separate declaring crate for construction accessor privacy tests.
use concord_core::reqwest::header::{HeaderName, HeaderValue};
const KEY: HeaderName = HeaderName::from_static("authorization");
concord_macros::api! { concord_core;
    pub client Sensitive(#[sensitive] origin: concord_core::BaseUrl) {
        base origin;
        GET ping();
    }
}
concord_macros::api! { concord_core;
    pub client Native(
        header("x-owned") owned: HeaderValue,
        header(KEY) conditional: String,
    ) {
        base "https://example.test";
        GET ping();
    }
}
