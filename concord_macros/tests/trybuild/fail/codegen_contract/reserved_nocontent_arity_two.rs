use concord_macros::api;

api! {
    client ReservedNoContentArityTwoApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> NoContent<A, B>
}

fn main() {}
