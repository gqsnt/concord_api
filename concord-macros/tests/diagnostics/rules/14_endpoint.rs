concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; #[inline] GET fetch(); }
}
fn main() {}
