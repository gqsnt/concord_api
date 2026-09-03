#[cfg(feature = "fixture-absence")]
use runtime::api;

#[cfg(feature = "fixture-absence")]
api! {
    client MultipartAbsentApi {
        timeout off;
        base "http://multipart-absent.example.test";


    POST create(body body: ::concord::body::MultipartBody using Multipart) -> () using NoContent;
}}

fn main() {}
