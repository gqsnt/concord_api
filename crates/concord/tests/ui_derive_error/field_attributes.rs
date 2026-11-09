#[derive(Debug)]
pub struct Payload;

#[derive(Debug, concord::derive::Error)]
pub enum Error {
    #[display("payload")]
    Skipped(#[from(skip)] Payload),
    #[display("named")]
    Named {
        #[display("{status}")]
        status: u16,
    },
}

fn main() {}
