use concord_macros::api;

api! {
    client ReservedNoContentArityOneApi {
        base "https://example.com"
    }

    GET Ping
        path ["ping"]
        -> NoContent<T>
}

fn main() {}
