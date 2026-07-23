use runtime::api;

api! {
    client MultipartAbsentApi {
        base "http://multipart-absent.example.test";
    }

    POST create(#[body] body: Multipart) -> NoContent;
}

fn main() {}
