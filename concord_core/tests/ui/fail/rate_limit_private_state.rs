use concord_core::advanced::{GovernorRateLimiter, RateLimitPermit, RateLimitPlan};

fn main() {
    let _ = RateLimitPermit { _private: () };
    let _ = RateLimitPlan { rules: Box::new([]) };
    let governor = GovernorRateLimiter::new();
    let _ = governor.state;
}
