#[derive(Debug, concord::Error)]
#[display(with = fmt_error)]
pub enum Error {
    Gated(Box<u32>),
}

fn fmt_error(error: &Error) -> String {
    format!("{error:?}")
}

fn main() {}
