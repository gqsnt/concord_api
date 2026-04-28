use concord_macros::api;

api! {
    client UiCacheUnknownProfileSuggestion {
        base https "example.com"

        cache short {
            ttl 60 seconds
        }
    }

    GET Ping
        cache shrot
        -> Json<()>;
}

fn main() {}

