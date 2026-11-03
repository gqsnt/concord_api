//! A downstream API whose server compresses JSON responses. reqwest's `gzip` feature, enabled by
//! this crate (not by Concord), decompresses the body; Concord's formats then see the response head
//! reqwest exposes and the decompressed bytes.
use concord::bytes::Bytes;
use concord::{Binary, CodecError, Decode, Json, ResponseHead};
use serde::Deserialize;

/// The decoded fixture.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Catalog {
    /// Items.
    pub items: Vec<Item>,
}

/// One item.
#[derive(Debug, Deserialize, PartialEq)]
pub struct Item {
    /// Identifier.
    pub id: u64,
    /// Name.
    pub name: String,
}

/// The head fields a format observed.
#[derive(Debug, PartialEq)]
pub struct Seen {
    /// `Content-Type`, as exposed.
    pub content_type: Option<String>,
    /// `Content-Encoding`, as exposed.
    pub content_encoding: Option<String>,
    /// `Content-Length`, as exposed.
    pub content_length: Option<String>,
}

/// `F`, also reporting the head it was given: `Json`'s check and decoder, unchanged.
pub struct Observed<F>(std::marker::PhantomData<fn() -> F>);

impl<T, F: Decode<T>> Decode<(T, Seen)> for Observed<F> {
    type Context = (F::Context, Seen);

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        F::check_content_type(head)
    }

    fn context(head: &ResponseHead<'_>) -> Result<Self::Context, CodecError> {
        let field = |name: &str| {
            head.headers()
                .get(name)
                .map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
        };
        let seen = Seen {
            content_type: field("content-type"),
            content_encoding: field("content-encoding"),
            content_length: field("content-length"),
        };
        Ok((F::context(head)?, seen))
    }

    fn decode_body((context, seen): Self::Context, body: Bytes) -> Result<(T, Seen), CodecError> {
        Ok((F::decode_body(context, body)?, seen))
    }
}

concord::api! {
    /// A catalog served compressed.
    pub client Shop {
        base "http://shop.invalid";
        GET catalog() -> Catalog { path "catalog"; response Json; }
        GET observed() -> (Catalog, Seen) { path "catalog"; response Observed<Json>; }
        GET raw() -> Bytes { path "catalog"; response Binary; }
    }
}
