// VALUE-03 / §7.1: the v2 `with_x` optional control is removed; the v3
// controls are `set_x`, `replace_x`, `set_x_if_some` and `omit_x`.
use concord::{Binary, api};

api! {
    client WithControlApi {
        timeout 30s;
        base "https://controls.example.test";
        values { locale?: String; }
        query { "locale" = locale; }

        GET read() -> concord::Bytes via Binary;
    }
}

fn main() {
    let _ = WithControlApi::builder().set_locale("fr");
    let _ = WithControlApi::builder().with_locale("fr".to_owned());
}
