//! The standard formats of Concord, written against the public contracts of
//! [`concord_core`]: [`Encode`](concord_core::Encode) and [`Decode`](concord_core::Decode).
//!
//! | Format | Feature | Body (`Encode<T>`) | Output (`Decode<T>`) |
//! |---|---|---|---|
//! | `Json` | `json` | any `T: Serialize`, `application/json` | any `T: DeserializeOwned` |
//! | [`Text`] | — | `String`, `text/plain; charset=utf-8` | `String` (UTF-8) |
//! | [`Binary`] | — | `Bytes`, `Vec<u8>`, `application/octet-stream` | `Bytes`, `Vec<u8>` |
//! | `Form` | `form` | any `T: Serialize` (flat), `application/x-www-form-urlencoded` | any `T: DeserializeOwned` |
//!
//! None of them is known to the `api!` macro: a statement names them by their type path
//! (`format Json;`), exactly like a format of your own. Applications normally get them through
//! the `concord` crate.

mod binary;
#[cfg(feature = "form")]
mod form;
#[cfg(feature = "json")]
mod json;
mod text;

pub use binary::Binary;
#[cfg(feature = "form")]
pub use form::Form;
#[cfg(feature = "json")]
pub use json::Json;
pub use text::Text;
