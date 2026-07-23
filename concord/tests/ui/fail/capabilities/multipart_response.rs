use concord::api;

api! {
    client MultipartApi {
        base "http://multipart.example.test";
    }

    scope uploads() {
        path ["uploads"];
        GET invalid() -> Multipart;
    }
}

fn main() {}
