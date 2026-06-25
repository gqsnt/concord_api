use concord_macros::api;

api! {
    client CacheZeroCapacityApi {
        base "https://example.com"

        cache bad {
            capacity 0 entries
        }
    }

    GET PingZero
        path ["ping-zero"]
        -> Json<String>
}

api! {
    client CacheOverflowCapacityApi {
        base "https://example.com"

        cache bad {
            capacity 18446744073709551616 entries
        }
    }

    GET PingOverflowCapacity
        path ["ping-overflow-capacity"]
        -> Json<String>
}

api! {
    client CacheZeroMaxBodyApi {
        base "https://example.com"

        cache bad {
            max_body 0 bytes
        }
    }

    GET PingZeroMaxBody
        path ["ping-zero-max-body"]
        -> Json<String>
}

api! {
    client CacheOverflowMaxBodyApi {
        base "https://example.com"

        cache bad {
            max_body 18446744073709551616 bytes
        }
    }

    GET PingOverflowMaxBody
        path ["ping-overflow-max-body"]
        -> Json<String>
}

api! {
    client CacheUnknownMaxBodyUnitApi {
        base "https://example.com"

        cache bad {
            max_body 1 frob
        }
    }

    GET PingUnknownMaxBodyUnit
        path ["ping-unknown-max-body-unit"]
        -> Json<String>
}

fn main() {}
