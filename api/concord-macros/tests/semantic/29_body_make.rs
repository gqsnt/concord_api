concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(code: String) { body make { "x-code" = "{*code}"; } } }
}
fn main() {}
