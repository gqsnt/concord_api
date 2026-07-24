#![allow(dead_code)]

const PROTECTED_INPUT: &str = "MSRV_PROTECTED_VALUE_SENTINEL";

#[path = "../../../../../crates/concord-core/tests/ui/fail/request_spec_non_send_input.rs"]
mod case;

fn main() {}
