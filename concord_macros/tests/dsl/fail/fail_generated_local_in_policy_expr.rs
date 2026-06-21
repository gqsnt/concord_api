use concord_core::prelude::*;
use concord_macros::api;

api! {
    client GeneratedLocalPolicyApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    headers {
        "X-Cx" = cx.token,
        "X-Ep" = ep.id,
        "X-Vars" = vars.token,
        "X-Self" = self.token,
        "X-Request" = request.url.to_string()
    }
    -> Text<String>
}

fn main() {}
