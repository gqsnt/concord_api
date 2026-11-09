#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("{1} failed")]
    Tuple(u16),
    #[display("{statsu} on {route}")]
    Named { status: u16, route: &'static str },
    #[display("HTTP {}")]
    Implicit(u32),
}

fn main() {}
