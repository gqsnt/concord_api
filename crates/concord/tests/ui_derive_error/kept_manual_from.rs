#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("{0}")]
    Concord(concord::Error),
}

impl From<concord::Error> for Error {
    fn from(error: concord::Error) -> Self {
        Error::Concord(error)
    }
}

fn main() {}
