// Formats are defaults on clients and scopes; an endpoint writes its exceptions in place.
concord::api! {
    pub client Request {
        base "http://x.test";
        POST note(body note: String) { request text; }
    }
}
concord::api! {
    pub client Response {
        base "http://x.test";
        GET version() -> String { response text; }
    }
}
concord::api! {
    pub client Twice {
        base "http://x.test";
        response json;
        response text;
        GET version() -> String;
    }
}
concord::api! {
    pub client AfterChild {
        base "http://x.test";
        GET version() -> text String;
        response json;
    }
}
fn main() {}
