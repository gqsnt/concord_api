// A body media type is checked by the macro: `type/subtype` tokens, optional `; name=value`
// parameters, visible ASCII, no surrounding whitespace.
concord::api! {
    pub client NoSlash {
        base "http://x.test";
        POST a(json("application json") b: String);
    }
}
concord::api! {
    pub client BadParameter {
        base "http://x.test";
        POST a(text("text/plain; charset") b: String);
    }
}
concord::api! {
    pub client NotAscii {
        base "http://x.test";
        request json;
        POST a(body("application/vnd.caf\u{e9}+json") b: String);
    }
}
concord::api! {
    pub client Default {
        base "http://x.test";
        request json(" application/json");
        POST a(body b: String);
    }
}
fn main() {}
