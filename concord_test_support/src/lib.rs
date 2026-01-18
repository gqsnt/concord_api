
mod assert;
mod mock;

pub use assert::*;
pub use mock::*;

use bytes::Bytes;
use serde::Serialize;

pub fn json_bytes<T: Serialize>(v: &T) -> Bytes {
    Bytes::from(serde_json::to_vec(v).expect("json encode"))
}