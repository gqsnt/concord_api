// Each marker is written once, without arguments, and never twice over the same value.

#[derive(concord::Debug)]
struct Duplicate {
    #[sensitive]
    #[sensitive]
    token: String,
}

#[derive(concord::Debug)]
struct Arguments {
    #[sensitive(always)]
    token: String,
}

#[derive(concord::Debug)]
struct Conflicting {
    #[sensitive]
    #[public]
    token: String,
}

#[derive(concord::Debug)]
#[sensitive]
struct Redundant {
    #[sensitive]
    token: String,
}

#[derive(concord::Debug)]
#[sensitive]
enum RedundantVariant {
    #[sensitive]
    Token(String),
}

fn main() {}
