#[derive(Debug)]
pub struct Doc;

#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("the provider refused")]
    #[from(concord::Error<Doc>)]
    Api(concord::Failure<Doc>),
}

fn main() {}
