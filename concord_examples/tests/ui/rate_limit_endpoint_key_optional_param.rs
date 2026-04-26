use concord_macros::api;

api! {
    client UiRateLimitEndpointKeyOptionalParam {
        base https "example.com"
        rate_limit regional {
                bucket method by [region, endpoint] {
                    10 / 1m
                }
        }
    }

    GET Ping(region?: String)
    -> Json<()>
    {
        rate_limit key region = region // ERROR: optional param
        rate_limit regional
    }
}

fn main() {}
