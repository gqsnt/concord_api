#[derive(Debug)]
pub struct Payload;

#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("{0}")]
    Concord(concord::Error),
    #[display("payload")]
    #[from(String)]
    Api(Payload),
}

fn main() {}
