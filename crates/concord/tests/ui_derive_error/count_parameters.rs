#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("{value:width$}")]
    Named { value: u16, width: usize },
    #[display("{0:.1$}")]
    Positional(f64, usize),
    #[display("{0:.*}")]
    Star(f64),
}

fn main() {}
