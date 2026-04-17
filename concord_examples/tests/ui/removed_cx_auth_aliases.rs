use concord_macros::api;

api! {
    client UiRemovedCxAuthAliases {
        scheme: https,
        host: "example.com",
        vars {
            tenant: String
        }
        secret {
            api_key: String
        }
        headers {
            "x-tenant" = cx.tenant,
            "x-api-key" = auth.api_key,
        }
    }
}

fn main() {}
