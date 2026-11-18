// Rust puts an identifier in NFC before the macro reads it: `Kite`, written with
// U+212A KELVIN SIGN, is the client `Kite` (docs/reference/reserved-names.json, spelling).
concord::api! {
    pub client Kite {
        base "https://a.test";
        GET fly();
    }
}

fn main() {
    let _ = Kite::new().fly();
    assert_eq!(<kite::Fly as kite::Endpoint>::NAME, "kite.fly");
}
