use concord_macros::api;

api! {
    client UiRateLimitPrecisionTooSmall {
        base https "example.com"
    }

    GET Ping
        rate_limit {
            bucket method by [host] {
                2000000001 / 1s // ERROR: sub-nanosecond cell period
            }
        }
    -> Json<()>
}

fn main() {}
