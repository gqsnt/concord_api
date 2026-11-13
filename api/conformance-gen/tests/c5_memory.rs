//! Original integration suite, run against the generated clients.
// The original's private common module cannot be re-exported to satisfy its
// absolute crate::common path, so this unchanged suite needs both includes.
#![allow(clippy::duplicate_mod)]
extern crate tour_gen as tour;

// The nested Linux test names this helper through the crate root.
#[path = "../../conformance/tests/common/mod.rs"]
mod common;

#[path = "../../conformance/tests/c5_memory.rs"]
mod original;
