use concord_macros::api;

api! {
    client UiRateLimitPrecisionTooSmall {
        scheme: https,
        host: "example.com",
    }

    GET Ping {
        rate_limit {
            bucket method by [route.host] {
                limit 2000000001 every 1 second // ERROR: sub-nanosecond cell period
            }
        }
        -> Json<()>;
    }
}

fn main() {}
