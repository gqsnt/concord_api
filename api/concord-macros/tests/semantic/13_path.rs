concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(code?: String) { path "records/{code}"; } }
}
fn main() {}
