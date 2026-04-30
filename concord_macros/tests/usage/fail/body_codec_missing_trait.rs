use concord_core::prelude::*;
use concord_macros::api;
use std::marker::PhantomData;

pub struct MissingBody<T>(PhantomData<T>);

#[derive(Clone)]
pub struct CreateUser;

api! {
    client MissingBodyCodecApi { base https "example.com" }

    POST Create(body: MissingBody<CreateUser>)
        as create
        path ["users"]
        -> Json<String>
}

fn main() {}
