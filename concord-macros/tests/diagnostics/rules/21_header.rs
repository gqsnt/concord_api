concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; headers { "x-mode" = "one"; } GET fetch() { headers { "X-Mode" = "two"; } } }
}
fn main() {}
