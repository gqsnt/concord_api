use concord_core::prelude::*;
use concord_macros::api;

api! {
    client SplitHttpApi { base http "example.com" }
}

fn main() {}
