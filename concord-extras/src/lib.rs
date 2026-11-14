//! What Concord ships beside its core.
//!
//! JSON, form and optional multipart writers, four readers, six pagers, HTTP Basic, and an adapter for
//! `tower`. Extensions use the public traits of `concord-core`. Body writers use
//! [`concord_core::body::Bounded`] and [`concord_core::body::WriteError`] to retain
//! safe input names and the original format error; your own writers use the same door.
//! Readers share [`concord_core::read::details`]; pagers share
//! [`concord_core::page::fragmentless`]. Both helpers are public extension points.
//! The Tower adapter also uses a hidden helper of the engine for the URL of an answer;
//! that helper is outside the extension API.
//!
//! People depend on the crate `concord-api`, which re-exports this one.
//!
//! | Module | What it holds | Written with |
//! |---|---|---|
//! | [`body`] | `json(x)`, `form(x)`, `multipart()` (feature `multipart`) | `concord_core::Body` |
//! | [`read`] | `Json`, `Text`, `Binary`, `Stream` | `concord_core::Reader` |
//! | [`page`] | `by_offset`, `by_offset_limit`, `by_page`, `by_cursor`, `by_link`, `by_next_url` | `concord_core::page::{Call, Slot}` |
//! | the root | `basic(user, password)` | `concord_core::Secret` |
//! | `tower` (feature `tower`) | `Tower` | `concord_core::Transport` |
//!
//! With the `multipart` feature, use `multipart()` in a `body` statement to send
//! text fields and file parts together.
//!
//! `text` and `file` return the builder; `finish()` returns `Result<Body, MultipartError>`.
//! Inputs in the description are references: `photo` is a `&Body`, accepted directly
//! by `file` through `From<&Body>`. It shares any one-shot stream with the input.
//!
//! ```
//! # #[cfg(feature = "multipart")] {
//! use concord_core::Body;
//! use concord_extras::{multipart, MultipartError};
//!
//! fn upload(name: &str, photo: &Body) -> Result<Body, MultipartError> {
//!     multipart().file("photo", name, "image/jpeg", photo).finish()
//! }
//! # assert!(upload("photo.jpg", &Body::from("photo")).unwrap().as_bytes().is_some());
//! # }
//! ```
//!
//! Buffered parts give deterministic bytes. Streaming parts use a random boundary
//! and are claimed together before sending.

#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod body;
#[cfg(feature = "multipart")]
pub use body::{multipart, Multipart, MultipartError};
pub mod page;
pub mod read;
#[cfg(feature = "tower")]
pub mod tower;

use concord_core::Secret;

/// The value of an `Authorization` header for HTTP Basic:
/// `headers { "authorization" = basic(user, password); }`.
///
/// It is a [`Secret`], so the header is marked sensitive and is never printed.
pub fn basic(user: impl Into<Secret>, password: impl Into<Secret>) -> Secret {
    let (user, password) = (user.into(), password.into());
    let pair = format!("{}:{}", &*user.expose(), &*password.expose());
    Secret::new(format!("Basic {}", base64(pair.as_bytes())))
}

/// Base64 with padding (RFC 4648, section 4).
fn base64(bytes: &[u8]) -> String {
    fn digit(bits: u32) -> char {
        // PANIC-SAFETY: Masking leaves only 0..63 before the u8 conversion.
        match (bits & 63) as u8 {
            // PANIC-SAFETY: The matched digit ranges keep every intermediate and resulting ASCII value within u8.
            n @ 0..=25 => char::from(b'A' + n),
            // PANIC-SAFETY: The matched digit ranges keep every intermediate and resulting ASCII value within u8.
            n @ 26..=51 => char::from(b'a' + n - 26),
            // PANIC-SAFETY: The matched digit ranges keep every intermediate and resulting ASCII value within u8.
            n @ 52..=61 => char::from(b'0' + n - 52),
            62 => '+',
            _ => '/',
        }
    }
    // PANIC-SAFETY: A slice length is at most isize::MAX; ceil(len/3)*4 fits usize on supported targets.
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    // PANIC-SAFETY: The chunk size is the nonzero constant three.
    for chunk in bytes.chunks(3) {
        let b = [
            *chunk.first().unwrap_or(&0),
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        // PANIC-SAFETY: b is a three-byte array; constant indexes are in range and all u32 shifts are below 32.
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        // PANIC-SAFETY: b is a three-byte array; constant indexes are in range and all u32 shifts are below 32.
        text.push(digit(n >> 18));
        // PANIC-SAFETY: b is a three-byte array; constant indexes are in range and all u32 shifts are below 32.
        text.push(digit(n >> 12));
        // PANIC-SAFETY: b is a three-byte array; constant indexes are in range and all u32 shifts are below 32.
        text.push(if chunk.len() > 1 { digit(n >> 6) } else { '=' });
        text.push(if chunk.len() > 2 { digit(n) } else { '=' });
    }
    text
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_is_the_one_of_the_rfc() {
        // RFC 4648, section 10
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(super::base64(plain.as_bytes()), coded);
        }
        // RFC 7617, section 2
        assert_eq!(
            super::base64(b"Aladdin:open sesame"),
            "QWxhZGRpbjpvcGVuIHNlc2FtZQ=="
        );
    }

    #[test]
    fn c8_2_basic_is_the_header_value_of_the_rfc() {
        // RFC 7617, section 2
        let value = super::basic("Aladdin", "open sesame");
        assert_eq!(&*value.expose(), "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==");
        assert_eq!(format!("{value:?}"), "Secret(…)");
    }
}
