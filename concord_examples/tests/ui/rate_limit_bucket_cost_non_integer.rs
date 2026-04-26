use concord_macros::api;

api! {
    client UiRateLimitBucketCostNonInteger {
        base https "example.com"
    }

    GET Ping
    -> Json<()>
    {
        rate_limit {
            bucket method by [host] {
                cost true // ERROR: expected integer literal
                30 / 10s
            }
        }
    }
}

fn main() {}
