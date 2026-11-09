#[derive(Debug, concord::derive::Error)]
#[display(with = fmt_error)]
pub enum Error {
    Gated(Box<u32>),
}

fn fmt_error(error: &Error) -> String {
    format!("{error:?}")
}

fn main() {}
