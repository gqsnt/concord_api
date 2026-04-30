use bytes::Bytes;
use concord_core::advanced::{BodyCodec, CodecError, EncodeContext, EncodedBody};
use concord_core::prelude::*;
use concord_macros::api;
use http::HeaderValue;
use std::marker::PhantomData;

use self::custom_codec_body_api::CustomCodecBodyApi;

pub struct Cbor<T>(PhantomData<T>);

#[derive(Clone)]
pub struct CreateUser {
    pub name: String,
}

impl BodyCodec for Cbor<CreateUser> {
    type Value = CreateUser;

    fn content_type() -> Option<HeaderValue> {
        Some(HeaderValue::from_static("application/cbor"))
    }

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::copy_from_slice(value.name.as_bytes())))
    }
}

api! {
    client CustomCodecBodyApi { base "https://example.com" }

    POST Create(body: Cbor<CreateUser>)
        as create
        path ["users"]
        -> Json<String>
}

fn usage(api: CustomCodecBodyApi) {
    let _ = api.create(CreateUser { name: "ada".to_string() });
}

fn main() {}
