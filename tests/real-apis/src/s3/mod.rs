//! `docs/examples/s3.md`: the description, the Rust beside it and the calls, as the page
//! prints them, with stand-ins for the data types that the page only names.
#![allow(missing_docs, dead_code, clippy::print_stdout, clippy::print_stderr)]

include!("stand_ins.rs");
#[cfg(not(test))]
include!("stand_in_wire.rs");
#[cfg(test)]
mod wire {
    use super::*;
    include!("wire.rs");
}
#[cfg(test)]
pub use wire::*;
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
