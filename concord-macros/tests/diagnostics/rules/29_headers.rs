concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch(code: String) { headers { "x-code" = "{*code}"; } } }
}
fn main() {}
