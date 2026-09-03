use concord::rate_limit::RateLimitKeyPart;

fn main() {
    let _ = RateLimitKeyPart::new("tenant", b"unframed");
}
