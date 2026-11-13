concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(prefix: String, rest: String) { path "{prefix}{*rest}"; } }
}
fn main() {}
