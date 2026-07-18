use concord_core::advanced::{RateLimitPermit, RateLimitPlan};

fn main() {
    let _ = RateLimitPermit { _private: () };
    let _ = RateLimitPlan { rules: Box::new([]) };
}
