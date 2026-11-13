//! The conformance suite of Concord: the tests that the engine must pass, written from the
//! paper before the engine exists.
//!
//! Every test is in `tests/`, and carries the number of the contract of
//! `docs/contracts.md` that it proves (`c2_3_…` proves C2.3). A test of code that is still
//! a stub carries `#[ignore = "waits for …"]`; it is written in full, and its expected
//! values are the paper's, word for word. They are not changed to make the code pass.
//!
//! | File | What it holds |
//! |---|---|
//! | `tests/c2_url.rs` | C2.1, C2.2, C2.3, C2.10: the base, the host, the path. |
//! | `tests/c2_values.rs` | C2.4 to C2.8, C2.13: the text of a value, the query, the headers. |
//! | `tests/c2_body.rs` | C2.9, C2.11, C2.12, C2.14 to C2.16: the body. |
//! | `tests/c2_table4.rs` | Table 4 of `docs/cases.md`: each shape of value in each place. |
//!
//! Two promises of C2.4 are about what does not compile, and are held here:
//!
//! A `Secret` has no `Serialize`, so `json(x)` of a type that holds one does not compile:
//!
//! ```compile_fail
//! #[derive(serde::Serialize)]
//! struct Task {
//!     title: String,
//!     key: concord::Secret,
//! }
//! let task = Task { title: "t".into(), key: concord::Secret::new("k") };
//! let _ = concord::json(task);
//! ```
//!
//! The same type without the secret compiles:
//!
//! ```
//! #[derive(serde::Serialize)]
//! struct Task {
//!     title: String,
//! }
//! let _ = concord::json(Task { title: "t".into() });
//! ```
//!
//! A list of secrets cannot be placed:
//!
//! ```compile_fail
//! use concord::__private::{AdaptValue as _, Piece, ValueRef};
//! let keys = vec![concord::Secret::new("a")];
//! let _ = Piece::Val("keys", &(&ValueRef(&keys)).adapt());
//! ```
//!
//! A secret, an optional secret, and references to them can:
//!
//! ```
//! use concord::__private::{AdaptValue as _, Piece, ValueRef};
//! let key = concord::Secret::new("a");
//! let optional = Some(concord::Secret::new("b"));
//! let _ = Piece::Val("key", &(&ValueRef(&key)).adapt());
//! let _ = Piece::Val("key", &(&ValueRef(&&key)).adapt());
//! let _ = Piece::Val("optional", &(&ValueRef(&optional)).adapt());
//! let _ = Piece::Val("optional", &(&ValueRef(&None::<concord::Secret>)).adapt());
//! ```
//!
//! C6.5: "`Kind`, `Class` and `Kept` are `#[non_exhaustive]`." A match outside the crate that
//! names every variant and has no `_` arm does not compile, for each of the three:
//!
//! ```compile_fail,E0004
//! use concord::error::Kind;
//! fn word(kind: Kind) -> &'static str {
//!     match kind {
//!         Kind::Build => "build",
//!         Kind::Transport => "transport",
//!         Kind::Refused => "refused",
//!         Kind::Mismatch => "mismatch",
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use concord::error::Class;
//! fn word(class: Class) -> &'static str {
//!     match class {
//!         Class::Timeout => "timeout",
//!         Class::Connect => "connect",
//!         Class::Body => "body",
//!         Class::Blocked => "blocked",
//!         Class::Other => "other",
//!     }
//! }
//! ```
//!
//! ```compile_fail,E0004
//! use concord::error::Kept;
//! fn word(kept: Kept) -> &'static str {
//!     match kept {
//!         Kept::Complete => "complete",
//!         Kept::Cut => "cut",
//!         Kept::Interrupted => "interrupted",
//!     }
//! }
//! ```
//!
//! The same matches with a `_` arm compile, so the three above fail for that reason alone:
//!
//! ```
//! use concord::error::{Class, Kept, Kind};
//! fn words(kind: Kind, class: Class, kept: Kept) -> [&'static str; 3] {
//!     [
//!         match kind {
//!             Kind::Build => "build",
//!             Kind::Transport => "transport",
//!             Kind::Refused => "refused",
//!             Kind::Mismatch => "mismatch",
//!             _ => "other",
//!         },
//!         match class {
//!             Class::Timeout => "timeout",
//!             Class::Connect => "connect",
//!             Class::Body => "body",
//!             Class::Blocked => "blocked",
//!             Class::Other => "other",
//!             _ => "other",
//!         },
//!         match kept {
//!             Kept::Complete => "complete",
//!             Kept::Cut => "cut",
//!             Kept::Interrupted => "interrupted",
//!             _ => "other",
//!         },
//!     ]
//! }
//! ```
//!
//! C9.5: a `TryInfo` "carries the clocks of its call [...], so it is `Clone` and not `Copy`":
//!
//! ```compile_fail,E0277
//! fn copy<T: Copy>() {}
//! copy::<concord::transport::TryInfo>();
//! ```
//!
//! ```
//! fn clone<T: Clone>() {}
//! clone::<concord::transport::TryInfo>();
//! ```

#![forbid(unsafe_code)]
