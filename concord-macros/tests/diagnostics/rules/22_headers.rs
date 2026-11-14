concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; GET fetch() { headers { override "x-mode" = 1; } } }
}
fn main() {}
