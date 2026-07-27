#![allow(dead_code)]

const PROTECTED_INPUT: &str = "MSRV_PROTECTED_VALUE_SENTINEL";

#[path = "../../../../../crates/concord/tests/ui/fail/rate_limits/rate_limit_missing_named_binding.rs"]
mod case;

fn main() {}
