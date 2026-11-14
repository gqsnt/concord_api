//! `docs/examples/slack.md`: the description, the Rust beside it and the calls, as the page
//! prints them, with stand-ins for the data types that the page only names.
#![allow(missing_docs, dead_code, clippy::print_stdout, clippy::print_stderr)]

include!("stand_ins.rs");
include!("description.rs");

mod beside {
    use super::*;
    include!("beside.rs");
}
pub use beside::*;

mod calling {
    use super::*;
    include!("calling.rs");
}
