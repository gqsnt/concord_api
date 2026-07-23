use concord_core::advanced::{RateLimitKey, RateLimitPlan};

fn inspect(key: RateLimitKey, plan: &RateLimitPlan) {
    let _ = format!("{key:?}");
    let _ = format!("{plan:?}");
}

fn main() {}
