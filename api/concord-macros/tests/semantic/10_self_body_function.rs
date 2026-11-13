concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; POST fetch() { body <Self as Maker>::make { "key" = 1; } } }
}
fn main() {}
