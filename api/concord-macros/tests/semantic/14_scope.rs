concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; #[repr(C)] scope fragment { GET fetch(); } }
}
fn main() {}
