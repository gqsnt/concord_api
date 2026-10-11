// VALUE-03: optional absence is selected with `omit_x` (or `replace_x(None)`);
// there is no `clear_x` value control.
use concord::{Text, api};

api! {
    client ClearControlApi {
        timeout 30s;
        base "https://example.test";
        values { locale?: String; }
        query { "locale" = locale; }

        GET read() -> String via Text;
    }
}

fn main() {
    let _ = ClearControlApi::builder().omit_locale();
    let _ = ClearControlApi::builder().clear_locale();
}
