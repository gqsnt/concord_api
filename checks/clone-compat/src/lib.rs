//! Downstream checks for the conditional `Clone` of generated endpoint requests (decision D08).
//!
//! `concord::api!` implements `Clone` for an endpoint request as
//! `impl Clone for X where for<'__concord> T: Clone, …` over each value and body type. A bound on a
//! concrete type that does not hold would be a "trivially false bound" error without the binder;
//! with it, rustc does not check the bound at the impl and the impl simply does not apply
//! (rust-lang/rust#48214). That is a compatibility risk, not a defect: these checks fail if a
//! toolchain stops accepting the pattern or starts making move-only requests `Clone`.
//!
//! - **Positive, runtime** (`tests/clone.rs`): requests whose values (required, optional,
//!   initialized, `each` collection) and body are `Clone` can be cloned, clones are independent,
//!   and both prepare to identical requests. A declaration with move-only values and body still
//!   compiles and prepares (the canary for the impl itself being rejected).
//! - **Negative, compile-fail** (the doctests below): each move-only case has a twin that differs
//!   only in using [`Cloneable`] instead of [`MoveOnly`] and must compile. A compile-fail case
//!   therefore fails because the type is not `Clone`, on every toolchain. The `E0599` annotation
//!   ("method exists but its trait bounds were not satisfied") is checked by nightly rustdoc only;
//!   stable rustdoc (1.85, 1.97.1) ignores error codes, so on stable the twins carry the reason.
//!
//! # Required path value
//!
//! ```compile_fail,E0599
//! use concord_clone_compat::{MoveOnly as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(id: V) { path "items/{id}"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get(V::new("a"));
//! let _ = request.clone();
//! ```
//! ```
//! use concord_clone_compat::{Cloneable as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(id: V) { path "items/{id}"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get(V::new("a"));
//! let _ = request.clone();
//! ```
//!
//! # Optional query value
//!
//! ```compile_fail,E0599
//! use concord_clone_compat::{MoveOnly as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(query note?: V) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get().set_note(V::new("a"));
//! let _ = request.clone();
//! ```
//! ```
//! use concord_clone_compat::{Cloneable as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(query note?: V) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get().set_note(V::new("a"));
//! let _ = request.clone();
//! ```
//!
//! # Initialized header value
//!
//! ```compile_fail,E0599
//! use concord_clone_compat::{MoveOnly as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(header("x-key") key: V = V::new("k")) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get();
//! let _ = request.clone();
//! ```
//! ```
//! use concord_clone_compat::{Cloneable as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(header("x-key") key: V = V::new("k")) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get();
//! let _ = request.clone();
//! ```
//!
//! # `each` collection of move-only items
//!
//! ```compile_fail,E0599
//! use concord_clone_compat::{MoveOnly as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(query("tag") each tags: Vec<V>) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get(vec![V::new("a")]);
//! let _ = request.clone();
//! ```
//! ```
//! use concord_clone_compat::{Cloneable as V};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     GET get(query("tag") each tags: Vec<V>) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).get(vec![V::new("a")]);
//! let _ = request.clone();
//! ```
//!
//! # Move-only body
//!
//! ```compile_fail,E0599
//! use concord_clone_compat::{MoveOnly as V, Raw};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     POST put(body payload: V via Raw) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).put(V::new("a"));
//! let _ = request.clone();
//! ```
//! ```
//! use concord_clone_compat::{Cloneable as V, Raw};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     POST put(body payload: V via Raw) { path "items"; } } }
//! let request = Api::new(concord::reqwest::Client::new()).put(V::new("a"));
//! let _ = request.clone();
//! ```
//!
//! # One move-only value among `Clone` ones
//!
//! ```compile_fail,E0599
//! use concord_clone_compat::{Cloneable as C, MoveOnly as V, Raw};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     POST put(id: C, query note?: C, header("x-nonce") nonce: V, body payload: C via Raw) {
//!         path "items/{id}";
//!     } } }
//! let request = Api::new(concord::reqwest::Client::new()).put(C::new("1"), V::new("n"), C::new("p"));
//! let _ = request.clone();
//! ```
//! ```
//! use concord_clone_compat::{Cloneable as C, Cloneable as V, Raw};
//! concord::api! { pub client Api { base "http://x.invalid";
//!     POST put(id: C, query note?: C, header("x-nonce") nonce: V, body payload: C via Raw) {
//!         path "items/{id}";
//!     } } }
//! let request = Api::new(concord::reqwest::Client::new()).put(C::new("1"), V::new("n"), C::new("p"));
//! let _ = request.clone();
//! ```

use std::fmt;

use concord::reqwest::header::HeaderValue;
use concord::{BodyError, Encode, ToHeaderValue};

/// A value that is never `Clone` (otherwise identical to [`Cloneable`]).
#[derive(Debug, PartialEq, Eq)]
pub struct MoveOnly(String);

/// A `Clone` value (otherwise identical to [`MoveOnly`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloneable(String);

/// A downstream body representation: the value's text as `text/plain`.
#[derive(Debug)]
pub struct Raw;

macro_rules! value {
    ($t:ident) => {
        impl $t {
            /// Wraps `text`.
            pub fn new(text: &str) -> Self {
                Self(text.to_owned())
            }
        }

        impl fmt::Display for $t {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        /// Header values use the text rule (`ToHeaderValue`), path and query values `Display`.
        impl ToHeaderValue for $t {
            fn to_header_value(&self) -> Option<HeaderValue> {
                self.0.to_header_value()
            }
        }

        impl Encode<$t> for Raw {
            const CONTENT_TYPE: Option<&'static str> = Some("text/plain");

            fn encode(value: $t) -> Result<concord::reqwest::Body, BodyError> {
                Ok(value.0.into())
            }
        }
    };
}

value!(MoveOnly);
value!(Cloneable);
