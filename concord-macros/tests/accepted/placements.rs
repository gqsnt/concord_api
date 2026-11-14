fn comma(values: &[String]) -> String { values.join(",") }
mod headers {
    use super::comma;
    concord::api! { pub client Legal { base "https://legal.test";
        scope things(comma?:bool) { headers { "x-comma"=comma; }
            GET list(tags:Vec<String>) { headers { "x-tags"=comma(tags); } }
        }
    } }
}
mod body {
    use super::comma;
    concord::api! { pub client Legal { base "https://legal.test";
        part options(comma?:bool) { body concord::json { "comma"=comma; } }
        part tagged(tags:Vec<String>) { body concord::json { "tags"=comma(tags); } }
        POST list(..options, ..tagged);
    } }
}
mod path {
    use super::comma;
    concord::api! { pub client Legal { base "https://legal.test";
        scope things(comma:String) { path "{comma}";
            GET list(tags:Vec<String>) { query { "tags"=comma(tags); } }
        }
    } }
}
fn main() {
    assert_eq!(headers::Legal::new().things().list(["a","b"]).request().unwrap().headers()["x-tags"], "a,b");
    assert_eq!(body::Legal::new().list(["a","b"]).request().unwrap().body().unwrap().as_bytes().unwrap(), br#"{"tags":"a,b"}"#);
    assert_eq!(path::Legal::new().things("scope").list(["a","b"]).request().unwrap().url().query(), Some("tags=a%2Cb"));
}
