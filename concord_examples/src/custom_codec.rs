use bytes::Bytes;
use concord_core::advanced::{
    BodyCodec, CodecError, DecodeContext, EncodeContext, EncodedBody, ResponseCodec,
};
use concord_macros::api;
use http::HeaderValue;
use std::marker::PhantomData;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CreateUser {
    pub name: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct User {
    pub id: u64,
    pub name: String,
}

pub struct Compact<T>(PhantomData<T>);

impl BodyCodec for Compact<CreateUser> {
    type Value = CreateUser;

    fn content_type() -> Option<HeaderValue> {
        Some(HeaderValue::from_static("application/x-concord-compact"))
    }

    fn encode(value: Self::Value, _ctx: EncodeContext<'_>) -> Result<EncodedBody, CodecError> {
        Ok(EncodedBody::from_bytes(Bytes::copy_from_slice(
            value.name.as_bytes(),
        )))
    }
}

impl ResponseCodec for Compact<User> {
    type Value = User;

    fn accept() -> Option<HeaderValue> {
        Some(HeaderValue::from_static("application/x-concord-compact"))
    }

    fn decode(bytes: Bytes, _ctx: DecodeContext<'_>) -> Result<Self::Value, CodecError> {
        let text = std::str::from_utf8(&bytes)
            .map_err(|source| CodecError::with_source("compact response is not utf-8", source))?;
        let (id, name) = text
            .split_once(':')
            .ok_or_else(|| CodecError::new("compact response must be `id:name`"))?;
        let id = id
            .parse::<u64>()
            .map_err(|source| CodecError::with_source("compact id is not an integer", source))?;
        Ok(User {
            id,
            name: name.to_string(),
        })
    }
}

api! {
    client CustomCodecApi {
        base "https://example.com"
    }

    POST CreateUser(body: Compact<CreateUser>)
        as create_user
        path ["users"]
        -> Compact<User>
}

pub use self::custom_codec_api::CustomCodecApi;
