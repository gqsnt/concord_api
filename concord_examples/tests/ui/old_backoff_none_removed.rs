use concord_macros::api;

api! {
    client OldBackoffNoneSyntax {
        base https "example.com"

        retry read {
            attempts 2
            methods [GET]
            on [500]
            backoff none
        }
    }
}

fn main() {}
