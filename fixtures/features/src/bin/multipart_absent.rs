#[cfg(feature = "fixture-absence")]
use runtime::api;

#[cfg(feature = "fixture-absence")]
api! {
    client MultipartAbsentApi {
        timeout 30s;
        base "http://multipart-absent.example.test";


    POST create(body body: ::concord::body::MultipartBody via Multipart) -> () via Empty;
}}

fn main() {}
