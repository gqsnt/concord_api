concord::api! { client B { base "https://b.example"; POST go(milestone?:Option<u64>) {query {milestone;}} } }
fn main() { let _ = B::new().go().milestone(None); }
