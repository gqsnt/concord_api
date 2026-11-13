#![allow(dead_code, unused_imports)]
// Rule 4: "an input hides an item of its name in the lines of ITS node". Here the item is used in the lines of a node BELOW.
pub mod hides_below {
    pub fn comma(list: &[String]) -> String { list.join(",") }
    concord::api! {
        pub client Legal {
            base "https://l.example";
            scope things(comma?: bool) {
                query { comma; }                                   // the scope places its input `comma`
                GET list(tags: Vec<String>) { query { "tags" = comma(tags); } }   // the endpoint calls the function `comma`
            }
        }
    }
}
// the same description, with the scope's line taken away from `query` (placed in headers): control
pub mod control {
    pub fn comma(list: &[String]) -> String { list.join(",") }
    concord::api! {
        pub client Legal {
            base "https://l.example";
            scope things(comma?: bool) {
                headers { "x-comma" = comma; }
                GET list(tags: Vec<String>) { query { "tags" = comma(tags); } }
            }
        }
    }
}

fn main() { let r=hides_below::Legal::new().things().list(["a", "b"]).request().unwrap(); assert_eq!(r.url().query(),Some("tags=a%2Cb")); }
