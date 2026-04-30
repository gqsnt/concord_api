use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use concord_macros::api;
use std::marker::PhantomData;

use self::custom_codec_both_api::CustomCodecBothApi;

pub struct Cbor<T>(PhantomData<T>);

#[derive(Clone)]
pub struct CreateUser {
    pub name: String,
}

#[derive(Clone)]
pub struct User {
    pub id: u64,
}

impl BodyCodec for Cbor<CreateUser> {
    type Value = CreateUser;

    fn content_type() -> &'static str {
        "application/cbor"
    }

    fn encode(value: &Self::Value, _ctx: EncodeContext) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::copy_from_slice(value.name.as_bytes()))
            .with_content_type(Self::content_type()))
    }
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
    client CustomCodecBothApi { base https "example.com" }

    POST Create(body: Cbor<CreateUser>)
        as create
        path ["users"]
        -> Cbor<User>
}

fn usage(api: CustomCodecBothApi) {
    let _ = api.create(CreateUser { name: "ada".to_string() });
}

fn main() {}
