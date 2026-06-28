use concord_macros::api;

api! {
    client ReservedRecordsUnsupportedApi { base "https://example.com" }

    GET Events
        path ["events"]
        -> Records<String, String>
}

fn main() {}
