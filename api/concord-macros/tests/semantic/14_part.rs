concord::api! {
    client Workshop { base "https://workshop.test"; read Reader; #[cfg(feature="extra")] part fragment() {} GET fetch(..fragment); }
}
fn main() {}
