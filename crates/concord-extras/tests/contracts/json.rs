#![cfg(feature = "json")]

use concord_core::contracts::{
    Bytes, RequestRepresentation, ResponseDriver, ResponseRepresentation,
};
use concord_extras::codec::Json;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
struct Message {
    id: u32,
}

#[test]
fn json_uses_the_public_representation_contract() {
    fn request_representation<R: RequestRepresentation<Message>>() {}
    fn response_representation<R: ResponseRepresentation<Message>>() {}
    request_representation::<Json>();
    response_representation::<Json>();
}

#[test]
fn json_selects_the_core_buffered_driver() {
    fn buffered<R>()
    where
        R: ResponseRepresentation<Message>,
        <R as ResponseRepresentation<Message>>::Driver: ResponseDriver<Body = Bytes>,
    {
    }
    buffered::<Json>();
}
