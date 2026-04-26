use concord_macros::api;

api! {
    client CacheRevalidateRequiresBool {
        base https "example.com"
        default {
            cache strict
        }
        cache strict {
                ttl 60 seconds
                revalidate // ERROR: must be explicit bool
        }
    }

    GET Cached
    -> Json<String>
    {
        path ["cached"]
    }
}

fn main() {}
