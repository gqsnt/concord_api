concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { headers { remove "x-mode"; } } }
}
fn main() {}
