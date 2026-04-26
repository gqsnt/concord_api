use concord_macros::api;

api! {
    client UiRateLimitUnknownKey {
        base https "example.com"
    }

    GET Ping
    -> Json<()>
    {
        rate_limit {
            bucket method by [region, endpoint] { // ERROR: unknown key
                30 / 10s
            }
        }
    }
}

fn main() {}
