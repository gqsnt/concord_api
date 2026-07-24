#![allow(dead_code)]

const PROTECTED_INPUT: &str = "MSRV_PROTECTED_VALUE_SENTINEL";

#[path = "../../../../../crates/concord-core/tests/ui/fail/dangerous_raw_response_absent.rs"]
mod case;

fn main() {}
