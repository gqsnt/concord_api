//! What generated code is made of. A description never needs this module.
//!
//! **Not public API.** These items are `pub` because generated code is code in the user's own
//! crate, and for no other reason: they can change in any release. (`serde` and `tokio` keep
//! the support of their macros in the same way.)

mod answer;
mod build;
mod call;
mod client;
mod data;
mod page;
mod run;
mod value;

pub use answer::{
    answer_empty, answer_head, answer_read, arm, cut_details, frame_answer, no_arm, read_body,
    reader, reader_for_problem, reader_for_response, refused, refused_plain, CheckedReader,
    PlainRefusal, ProblemRefusal, Refusal,
};
pub use build::{build_error, evaluate_body, evaluate_entry, prepare_call, Draft, Piece, Written};
pub use call::{
    prepare_conditional, Call, ConditionalAccess, ConditionalCall, Prepared, RequestSecrets,
};
pub use client::{Core, Options};
pub use data::{CallData, InputSlot};
#[cfg(test)]
use page::same_page_request;
pub use page::{
    pages, pages_from_macro, pages_of_pager, pages_with_pager, without_fragment, ClosureFromMacro,
    SecretUrl,
};
pub use run::Run;
pub use value::{
    idempotent, inherited, inherited_path, AdaptValue, BodySecretValue, BoolInput, Idempotent,
    InheritAcrossBase, InheritPathAcrossBase, SecretInput, SecretValue, Value, ValueRef,
};

pub use crate::retry::{again, Outcome};
