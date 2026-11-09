#[derive(concord::Error)]
pub enum Error {
    #[display("{0}")]
    Concord(concord::Error),
}

fn main() {}
