concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; status { 2xx { body } => unpack(body) } DELETE fetch(); }
}
fn main() {}
