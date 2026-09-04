#[cfg(feature = "fixture-absence")]
use runtime::api;

#[cfg(feature = "fixture-absence")]
api! {
    client JsonAbsentApi {
        timeout 30s;
        base "http://json-absent.example.test";


    POST create(
        body body: Vec<u32> via Json,
    ) -> Vec<u32> via Json;
}}

fn main() {}
