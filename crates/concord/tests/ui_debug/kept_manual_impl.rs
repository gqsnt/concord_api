// The derive added while the hand-written formatter is still there.
use std::fmt;

#[derive(concord::Debug)]
struct Token {
    #[sensitive]
    value: String,
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

fn main() {}
