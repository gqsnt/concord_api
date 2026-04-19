use concord_macros::api;

api! {
    client CacheRevalidateRequiresBool {
        scheme: https,
        host: "example.com",
        cache {
            profile strict {
                ttl 60 seconds
                revalidate // ERROR: must be explicit bool
            }
            default strict
        }
    }

    GET Cached
    -> Json<String>
    {
        path["cached"]
    }
}

fn main() {}
