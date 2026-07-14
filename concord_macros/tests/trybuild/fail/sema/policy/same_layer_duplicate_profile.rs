use concord_macros::api;

api! {
    client DuplicateProfileApi {
        base "https://example.com"

        profiles {
            profile read {}
        }

        default {
            profile read
            profile read
        }
    }
}

fn main() {}
