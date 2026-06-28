use concord_macros::api;

api! {
    client ReservedRecordsWrongArityApi { base "https://example.com" }

    GET Events
        path ["events"]
        -> Records<String>
}

fn main() {}
