#[cfg(feature = "fixture-absence")]
use runtime::api;

#[cfg(feature = "fixture-absence")]
api! {
    client MultipartAbsentApi {
        base "http://multipart-absent.example.test";
    }

    POST create(#[body] body: Multipart) -> NoContent;
}

fn main() {}
