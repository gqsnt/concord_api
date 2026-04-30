use concord_macros::api;
use std::marker::PhantomData;

pub struct MissingResponse<T>(PhantomData<T>);

#[derive(Clone)]
pub struct User;

api! {
    client MissingResponseCodecApi { base https "example.com" }

    GET GetUser
        as get_user
        path ["users", "me"]
        -> MissingResponse<User>
}

fn main() {}
