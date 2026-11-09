// The marker written with std's derive: nothing would withhold the value, so it is refused.

#[derive(Debug)]
struct Token {
    #[sensitive]
    value: String,
}

fn main() {}
