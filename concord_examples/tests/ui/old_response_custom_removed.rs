use concord_macros::api;

struct RiotRateLimitHeaders;

api! {
    client OldResponseCustomSyntax {
        base https "example.com"
        response custom RiotRateLimitHeaders
    }
}

fn main() {}
