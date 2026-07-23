use concord::api;

api! {
    client ReferenceApi {
        base "https://reference.example.test";
    }

    GET item() -> Bytes {
        path [ep.missing];
    }
}

fn main() {}
