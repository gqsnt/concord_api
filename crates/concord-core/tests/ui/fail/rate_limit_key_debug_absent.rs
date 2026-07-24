use concord_core::contracts::{RateLimitKey, RateLimitPlan};

fn inspect(key: RateLimitKey, plan: &RateLimitPlan) {
    let _ = format!("{key:?}");
    let _ = format!("{plan:?}");
}

fn main() {}
