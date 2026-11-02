// The format is no longer written in the payload position: `json b: T`, `F("m") b: T`,
// `body("m") b: T`, `Path b: T`, `-> json T`, `-> Path T` and `request F("m");` are errors whose
// message names the replacement (`body b: T` / `-> T` plus a `request` / `response` statement).
pub struct Item;
pub struct Csv;
concord::api! {
    pub client WordBody {
        base "http://x.test";
        POST a(json b: Item);
    }
}
concord::api! {
    pub client WordBodyWithMedia {
        base "http://x.test";
        POST a(json("application/merge-patch+json") b: Item);
    }
}
concord::api! {
    pub client BodyWithMedia {
        base "http://x.test";
        request json;
        POST a(body("application/problem+json") b: Item);
    }
}
concord::api! {
    pub client PathBody {
        base "http://x.test";
        POST a(Csv b: Item);
    }
}
concord::api! {
    pub client WordOutput {
        base "http://x.test";
        GET a() -> json Item;
    }
}
concord::api! {
    pub client PathOutput {
        base "http://x.test";
        GET a() -> Csv Item;
    }
}
concord::api! {
    pub client ParenthesizedMedia {
        base "http://x.test";
        request json("application/vnd.api+json");
        POST a(body b: Item);
    }
}
concord::api! {
    pub client ParenthesizedMediaOnTheEndpoint {
        base "http://x.test";
        POST a(body b: Item) { request text("text/markdown"); }
    }
}
fn main() {}
