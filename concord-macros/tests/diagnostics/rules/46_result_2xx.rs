concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; status { 2xx { body } => unpack(body) } GET fetch() -> Result<Record,Issue>; }
}
fn main() {}
