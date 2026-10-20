// §9.5: `Multipart` is a request-only representation; it is not a response
// representation.
use concord::api;
use concord::body::Multipart;

api! {
    client MultipartApi {
        timeout 30s;
        base "http://multipart.example.test";

        scope uploads() {
            path "uploads";

            GET invalid() -> () via Multipart;
        }
    }
}

fn main() {}
