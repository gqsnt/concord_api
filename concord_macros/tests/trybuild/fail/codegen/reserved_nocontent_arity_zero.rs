use concord_macros::api;

api! {
    client ReservedNoContentArityZeroApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> NoContent<>
}

fn main() {}
