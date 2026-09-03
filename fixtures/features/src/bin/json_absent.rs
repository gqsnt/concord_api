#[cfg(feature = "fixture-absence")]
use runtime::api;

#[cfg(feature = "fixture-absence")]
api! {
    client JsonAbsentApi {
        timeout off;
        base "http://json-absent.example.test";


    POST create(
        body body: Vec<u32> using Json<Vec<u32>>,
    ) -> Vec<u32> using Json<Vec<u32>>;
}}

fn main() {}
