concord::api! {
    client Workshop { base "https://workshop.test"; GET fetch() -> Record { status { 2xx { body } => body } } }
}
fn main() {}
