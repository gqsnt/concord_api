#[derive(Debug)]
pub struct Cause;

impl std::fmt::Display for Cause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("cause")
    }
}

impl std::error::Error for Cause {}

#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("one")]
    #[source]
    One(Cause),
    #[display("two")]
    Two(#[source] Cause, #[source] Cause),
    #[display("three")]
    Three(#[source(transparent)] Cause),
    #[display("four")]
    #[from(concord::Error<Cause>)]
    Four { cause: Cause },
}

#[derive(Debug, concord::derive::Error)]
#[source]
pub enum OnEnum {
    #[display("cause")]
    Cause(Cause),
}

fn main() {}
