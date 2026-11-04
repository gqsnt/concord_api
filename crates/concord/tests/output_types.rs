//! F02 (4.2): an output type is any Rust type, type macros included (`-> payload!()`, `[]`, `{}`,
//! qualified, `self::`/`crate::` and raw-identifier paths, nested in other types), checked by rustc
//! like a function's return type. Cross-crate absolute macro paths: `checks/clone-hygiene`.
#![allow(missing_docs)]

use concord::{CodecError, Decode, Encode, Prepared, ResponseHead, bytes::Bytes, reqwest};

#[derive(Debug, Default, PartialEq)]
pub struct Item(pub u8);

#[derive(Debug, Default, PartialEq)]
pub struct Wrapper<T>(pub T);

pub type Alias = Item;

/// Decodes anything with a `Default` (the tests only prepare requests).
pub struct Fill;
impl<T: Default> Decode<T> for Fill {
    type Context = ();
    fn check_content_type(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body(_: (), _: Bytes) -> Result<T, CodecError> {
        Ok(T::default())
    }
}

/// Encodes an `Item` as its number.
pub struct Number;
impl Encode<Item> for Number {
    const CONTENT_TYPE: Option<&'static str> = None;
    fn encode(value: Item) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.0.to_string()))
    }
}

macro_rules! item {
    () => {
        Item
    };
}
macro_rules! list {
    ($t:ty) => {
        Vec<$t>
    };
}

pub mod m {
    macro_rules! payload {
        () => {
            $crate::Item
        };
    }
    pub(crate) use payload;
}

pub mod r#mod {
    macro_rules! payload {
        () => {
            $crate::Wrapper<$crate::Item>
        };
    }
    pub(crate) use payload;
}

concord::api! {
    pub client Outputs {
        base "http://outputs.invalid";
        response Fill;
        GET paren() -> item!();
        GET bracket() -> item![];
        GET brace() -> item! {};
        GET brace_block() -> item! {} { path "block"; }
        GET qualified() -> crate::m::payload!();
        GET self_path() -> self::m::payload![];
        GET raw_path() -> r#mod::payload!();
        GET nested() -> list!(item!());
        GET in_vec() -> Vec<item!()>;
        GET in_option() -> Option<m::payload!()>;
        GET in_tuple() -> (item!(), u8);
        GET generic() -> Wrapper<item!()>;
        GET alias() -> Alias;
        GET unit() -> ();
        GET array() -> [u8; 2];
        GET ordinary() -> Item;
        POST body(body value: item!()) -> item!() { request Number; }
    }
}

/// The declared output type is the payload type itself (the binding is type-checked).
#[test]
fn macro_outputs_are_the_macros_types() {
    let api = Outputs::new(reqwest::Client::new());
    let _: Prepared<Item> = api.paren().prepare().unwrap();
    let _: Prepared<Item> = api.bracket().prepare().unwrap();
    let _: Prepared<Item> = api.brace().prepare().unwrap();
    let p: Prepared<Item> = api.brace_block().prepare().unwrap();
    assert_eq!(p.request().url().path(), "/block");
    let _: Prepared<Item> = api.qualified().prepare().unwrap();
    let _: Prepared<Item> = api.self_path().prepare().unwrap();
    let _: Prepared<Wrapper<Item>> = api.raw_path().prepare().unwrap();
    let _: Prepared<Vec<Item>> = api.nested().prepare().unwrap();
    let _: Prepared<Vec<Item>> = api.in_vec().prepare().unwrap();
    let _: Prepared<Option<Item>> = api.in_option().prepare().unwrap();
    let _: Prepared<(Item, u8)> = api.in_tuple().prepare().unwrap();
    let _: Prepared<Wrapper<Item>> = api.generic().prepare().unwrap();
    let _: Prepared<Item> = api.alias().prepare().unwrap();
    let _: Prepared<()> = api.unit().prepare().unwrap();
    let _: Prepared<[u8; 2]> = api.array().prepare().unwrap();
    let _: Prepared<Item> = api.ordinary().prepare().unwrap();
    let p: Prepared<Item> = api.body(Item(7)).prepare().unwrap();
    assert_eq!(
        p.request().body().unwrap().as_bytes(),
        Some(b"7".as_slice())
    );
}
