use concord::api;

api! {
    client ClearControlApi {
        timeout 30s;
        base "https://example.test";
        values { locale?: String; }

        GET read() -> String via Text;
    }
}

fn main() {
    let _ = ClearControlApi::builder().clear_locale();
}
