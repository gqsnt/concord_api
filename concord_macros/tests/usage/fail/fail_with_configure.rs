use concord_core::prelude::*;
use concord_macros::api;
use self::usage_with_configure_api::UsageWithConfigureApi;

api! {
    client UsageWithConfigureApi {
        base https "example.com"
    }

    GET Ping
        path ["ping"]
        -> Json<String>
}

fn bad_usage() {
    let _ = UsageWithConfigureApi::new().with_configure(|_cfg| {});
}

fn main() {}
