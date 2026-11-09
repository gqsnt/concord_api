#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("failed after {0} tries")]
    Exhausted(#[source] u32),
}

fn main() {}
