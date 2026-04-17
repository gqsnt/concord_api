use concord_macros::api;

api! {
    client UiRateLimitEndpointKeyOptionalParam {
        scheme: https,
        host: "example.com",
        rate_limit {
            profile regional {
                bucket method by [region, endpoint] {
                    limit 10 every 1 minute
                }
            }
        }
    }

    GET Ping {
        params { region?: String }
        rate_limit key region = region // ERROR: optional param
        rate_limit regional
        -> Json<()>;
    }
}

fn main() {}
