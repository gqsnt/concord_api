#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("{a-b} failed")]
    Dashed { a: u16 },
    #[display("{type}")]
    Keyword { kind: u16 },
    #[display("{1x}")]
    Digits(u16),
}

fn main() {}
