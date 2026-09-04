use concord::{api, Binary};

api! {
    client WithControlApi {
        timeout 30s;
        base "https://controls.example.test";
        values { locale?: String; }

        GET read() -> concord::Bytes via Binary;
    }
}

fn main() {
    let _ = WithControlApi::builder().with_locale("fr".to_owned());
}
