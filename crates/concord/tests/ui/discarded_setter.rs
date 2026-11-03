#![deny(unused_must_use)]

concord::api! {
    pub client Api(query lang?: String) {
        base "http://api.invalid";
        scope v2() {
            GET get();
        }
    }
}

fn main() {
    let api = Api::new(concord::reqwest::Client::new());
    // A setter consumes its receiver and returns the updated handle: on a clone (or a
    // temporary) the change is silently lost without the lint.
    api.clone().set_lang("de");
    api.clone().maybe_lang(None);
    api.v2();
    let _kept = api.set_lang("fr");
}
