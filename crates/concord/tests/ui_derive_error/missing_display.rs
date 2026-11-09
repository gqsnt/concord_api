#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("{0}")]
    Concord(concord::Error),
    Local(u16),
}

fn main() {}
