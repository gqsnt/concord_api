use concord::{api, Empty};

api! {
    client LiteralValues {
        base "http://example.test/";
        headers { "X-Test" = "\t café"; }
        query { "controls" = "\0\r\n\u{7f}"; }
        scope nested() {
            headers replace { "X-Test" = concat ["", '\t', "é"]; }
            GET fetch(value: String) -> () via Empty {
                headers {
                    replace "X-Test" = "";
                    append "X-Other" = concat ["前", value, "\t"];
                    "X-Unknown" = 'é';
                }
                query replace { "controls" = concat ["\0", '\n', true, -42]; }
            }
        }
    }
}

fn main() {
    let client = LiteralValues::new().unwrap();
    let _ = client.nested().fetch("value").plan().unwrap();
}
