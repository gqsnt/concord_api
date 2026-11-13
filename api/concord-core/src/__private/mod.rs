//! What generated code is made of. A description never needs this module.
//!
//! **Not public API.** These items are `pub` because generated code is code in the user's own
//! crate, and for no other reason: they can change in any release. (`serde` and `tokio` keep
//! the support of their macros in the same way.)

mod answer;
mod build;
mod call;
mod client;
mod page;
mod run;
mod value;

pub use answer::{
    arm, cut_details, no_arm, read_body, reader, refused, refused_plain, CheckedReader,
};
pub use build::{build_error, Draft, Piece, Written};
pub use call::{Call, Prepared, RequestSecrets};
pub use client::{Core, Options};
pub use page::{pages, same_page_request, without_fragment, SecretUrl};
pub use run::{exchange, exchange_raw, Run};
pub use value::{
    idempotent, inherited, inherited_path, AdaptValue, BodySecretValue, BoolInput, Idempotent,
    InheritAcrossBase, InheritPathAcrossBase, SecretInput, SecretValue, Value, ValueRef,
};

pub use crate::call_methods;
pub use crate::retry::{again, Outcome};
