use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, ContentType, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use concord_macros::api;
use std::marker::PhantomData;

use self::custom_codec_both_api::CustomCodecBothApi;

pub struct Cbor<T>(PhantomData<T>);

pub struct CborContentType;

impl ContentType for CborContentType {
    const CONTENT_TYPE: &'static str = "application/cbor";
}

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
    type Content = CborContentType;

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::copy_from_slice(value.name.as_bytes())))
    }
}

impl ResponseCodec for Cbor<User> {
    type Value = User;
    type Content = CborContentType;

    fn decode(_bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        Ok(User { id: 7 })
    }
}

api! {
    client CustomCodecBothApi { base "https://example.com" }

    POST Create(body: Cbor<CreateUser>)
        as create
        path ["users"]
        -> Cbor<User>
}

fn usage(api: CustomCodecBothApi) {
    let _ = api.create(CreateUser { name: "ada".to_string() });
}

fn main() {}
