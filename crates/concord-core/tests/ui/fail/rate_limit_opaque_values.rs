use concord_core::contracts::{RateLimitPermit, RateLimitPlan};

fn main() {
    let _ = RateLimitPermit { _private: () };
    let _ = RateLimitPlan { rules: Box::new([]) };
}
