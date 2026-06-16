use concord_macros::api;

api! {
    client UnknownRetryProfileEndpointApi {
        base "https://example.com"
    }

    GET Ping
    path ["ping"]
    retry missing
    -> Text<String>
}

fn main() {}
