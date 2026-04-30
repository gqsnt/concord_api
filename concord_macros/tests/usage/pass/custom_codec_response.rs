use bytes::Bytes;
use concord_core::advanced::{CodecError, DecodeContext, ResponseCodec};
use concord_macros::api;
use std::marker::PhantomData;

use self::custom_codec_response_api::CustomCodecResponseApi;

pub struct Cbor<T>(PhantomData<T>);

#[derive(Clone)]
pub struct User {
    pub id: u64,
}

impl ResponseCodec for Cbor<User> {
    type Value = User;

    fn accept() -> &'static str {
        "application/cbor"
    }

    fn decode(_bytes: &Bytes, _ctx: DecodeContext) -> Result<Self::Value, CodecError> {
        Ok(User { id: 7 })
    }
}

api! {
    client CustomCodecResponseApi { base https "example.com" }

    GET GetUser
        as get_user
        path ["users", "me"]
        -> Cbor<User>
}

fn usage(api: CustomCodecResponseApi) {
    let _ = api.get_user();
}

fn main() {}
