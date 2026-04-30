use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SplitHttpsApi { base https "example.com" }
}

fn main() {}
