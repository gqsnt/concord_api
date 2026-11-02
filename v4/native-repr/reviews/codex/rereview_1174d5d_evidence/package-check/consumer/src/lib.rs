//! A consumer of the packaged crates: formats (in place and defaults), a native header.
use concord::reqwest::header::HeaderValue;

pub static ACCEPT: HeaderValue = HeaderValue::from_static("application/json");

#[cfg(feature = "json")]
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq)]
pub struct Item {
    pub id: u64,
}

concord::api! {
    pub client Shop(#[sensitive] header("authorization") token: String, header("accept") accept: HeaderValue = &ACCEPT) {
        base "https://shop.example.test/v1";
        #[cfg(feature = "json")]
        scope json_api {
            request json;
            response json;
            GET item(id: u64) -> Item { path "items/{id}"; }
            POST create(body item: Item) -> Item { path "items"; }
        }
        GET health() -> text String { path "health"; }
        PUT upload(bytes("image/png") png: concord::bytes::Bytes) { path "image"; }
    }
}
