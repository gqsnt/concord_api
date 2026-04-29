use concord_core::prelude::*;
use concord_macros::api;

#[derive(Clone)]
struct CustomProvider;

api! {
    client CustomAuthApi {
        base https "example.com"
        credential c = custom<CustomProvider>(CustomProvider)
    }
}

fn main() {}
