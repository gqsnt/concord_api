// 12a/12b with the `json` and `form` features off: one error at each word written in place, one at
// each default that endpoints use (through its shared alias, whatever the number of users), and
// nothing for a default used only by cfg-disabled endpoints or by no endpoint at all.
pub struct Item;
concord::api! {
    pub client InPlace {
        base "http://x.test";
        GET a() -> json Item;
        GET b() -> json Item;
        POST login(form f: Item);
    }
}
concord::api! {
    pub client Defaults {
        base "http://x.test";
        request form;
        response json;
        GET a() -> Item;
        GET b() -> Item;
        POST c(body x: Item);
        POST d(body x: Item);
        scope disabled {
            response json;
            #[cfg(any())]
            GET only_user() -> Item;
        }
        scope unused {
            request form;
            response json;
            GET ping();
        }
    }
}
fn main() {}
