#![allow(dead_code)]

use concord_macros::api;

struct Result;
struct Option;
struct String;
struct Vec;
struct Method;
struct RequestHead;
mod core {}
mod runtime {}

api! {
    client AlphaApi {
        base "https://alpha.example.test";
        scope resources(scope_value: ::std::string::String) {
            path ["scopes", scope.scope_value, "resources"];
            GET ById(endpoint_value: ::std::string::String) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}

api! {
    client BetaApi {
        base "https://beta.example.test";
        scope resources(scope_value: ::std::string::String) {
            path ["scopes", scope.scope_value, "resources"];
            GET ById(endpoint_value: ::std::string::String) -> NoContent {
                path [ep.endpoint_value];
            }
        }
    }
}

pub fn proof() {
    let alpha = AlphaApi::new()
        .resources("alpha-scope".to_owned())
        .by_id("alpha-endpoint".to_owned());
    let beta = BetaApi::new()
        .resources("beta-scope".to_owned())
        .by_id("beta-endpoint".to_owned());
    assert_ne!(
        alpha.__concord_request_head().endpoint(),
        beta.__concord_request_head().endpoint()
    );
}
