use concord_macros::api;

api! {
    client UiRateLimitBucketCostZero {
        scheme: https,
        host: "example.com",
    }

    GET Ping {
        rate_limit {
            bucket method by [route.host] {
                cost 0 // ERROR: cost must be > 0
                limit 30 every 10 seconds
            }
        }
        -> Json<()>;
    }
}

fn main() {}
