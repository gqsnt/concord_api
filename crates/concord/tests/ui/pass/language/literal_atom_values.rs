// HTTP permits tabs and UTF-8 bytes above ASCII (including U+0085).
// Query values permit controls because URL encoding happens during planning.
use concord::{Binary, Bytes, api};

api! {
    client LiteralAtomValues {
        base "http://example.test/";
        headers {
            "x-empty" = "";
            "x-bool" = true;
            "x-integer" = -1_234;
            "x-tab" = "before\tafter";
            "x-unicode" = "café\u{85}";
            "x-char" = '\u{85}';
        }
        query { "controls" = "\0\r\n\u{7f}"; }
        scope nested(value: String) {
            headers {
                replace "x-tab" = concat ["prefix\t", value, concat ['\t', "suffix"]];
            }
            query { append "controls" = '\n'; }
            GET read(value: String) -> Bytes via Binary {
                headers replace {
                    "x-concat" = concat [nested.value, "é", concat ['\u{85}', value]];
                }
                query replace {
                    "concat" = concat ["\0", nested.value, concat ['\r', value]];
                }
            }
        }
    }
}

fn main() {
    let client = LiteralAtomValues::new().expect("client");
    client
        .nested("scope")
        .read("endpoint")
        .plan()
        .expect("plan");
}
