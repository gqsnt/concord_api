#[cfg(feature = "loopback-compat")]
mod assert;
#[cfg(feature = "loopback-compat")]
mod compatibility_loopback;
#[cfg(feature = "dangerous-dev-tools")]
mod deterministic;
#[cfg(feature = "dangerous-dev-tools")]
mod deterministic_assert;

#[cfg(feature = "loopback-compat")]
pub use assert::*;
#[cfg(feature = "loopback-compat")]
pub use compatibility_loopback::*;
#[cfg(feature = "dangerous-dev-tools")]
pub use deterministic::*;
#[cfg(feature = "dangerous-dev-tools")]
pub use deterministic_assert::*;

use bytes::Bytes;
use serde::Serialize;

pub fn json_bytes<T: Serialize>(v: &T) -> Bytes {
    Bytes::from(serde_json::to_vec(v).expect("json encode"))
}
