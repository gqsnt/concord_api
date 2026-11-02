// A body media type is checked by the macro: `type/subtype` tokens, optional `; name=value`
// parameters, visible ASCII, no surrounding whitespace.
concord::api! {
    pub client NoSlash {
        base "http://x.test";
        POST a(body b: String) { request json "application json"; }
    }
}
concord::api! {
    pub client BadParameter {
        base "http://x.test";
        POST a(body b: String) { request text "text/plain; charset"; }
    }
}
concord::api! {
    pub client NotAscii {
        base "http://x.test";
        POST a(body b: String) { request json "application/vnd.caf\u{e9}+json"; }
    }
}
concord::api! {
    pub client Default {
        base "http://x.test";
        request json " application/json";
        POST a(body b: String);
    }
}
fn main() {}
