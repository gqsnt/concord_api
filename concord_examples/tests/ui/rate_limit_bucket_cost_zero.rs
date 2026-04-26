use concord_macros::api;

api! {
    client UiRateLimitBucketCostZero {
        base https "example.com"
    }

    GET Ping
    -> Json<()>
    {
        rate_limit {
            bucket method by [host] {
                cost 0 // ERROR: cost must be > 0
                30 / 10s
            }
        }
    }
}

fn main() {}
