// A type-level marker written above the derive that introduces it.

#[sensitive]
#[derive(concord::Debug)]
struct Grant {
    token: String,
}

fn main() {}
