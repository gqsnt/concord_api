concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; HEAD fetch() -> bool { status { 2xx { body } => true } } }
}
fn main() {}
