#[derive(Debug)]
pub struct ProtocolError;

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("protocol")
    }
}

#[derive(Debug, concord::Error)]
pub enum Error {
    #[display("{0}")]
    Protocol(Box<ProtocolError>),
    #[display("again: {0}")]
    Again(ProtocolError),
}

fn main() {}
