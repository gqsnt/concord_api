#[derive(Debug)]
pub struct Doc;

/// Holds the answer, but cannot be built from the provider's failure.
#[derive(Debug)]
pub struct Answer;

impl std::fmt::Display for Answer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("answer")
    }
}

#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("{0}")]
    Concord(concord::Error),
    #[display("{0}")]
    #[from(concord::Error<Doc>)]
    Api(Box<Answer>),
}

fn main() {}
