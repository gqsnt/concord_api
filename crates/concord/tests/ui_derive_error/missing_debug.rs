#[derive(concord::derive::Error)]
pub enum Error {
    #[display("{0}")]
    Concord(concord::Error),
}

fn main() {}
