// Representation statements on an endpoint need something to apply to: `request` needs a body,
// `response` a typed output (`-> T`), `format` at least one of the two. An owner declares each
// direction at most once (`format` counts as both) and its statements precede its children.
concord::api! {
    pub client RequestWithoutBody {
        base "http://x.test";
        GET version() -> String { request concord::Text; response concord::Text; }
    }
}
concord::api! {
    pub client ResponseWithoutOutput {
        base "http://x.test";
        POST note(body note: String) { request concord::Text; response concord::Text; }
    }
}
concord::api! {
    pub client FormatWithNothing {
        base "http://x.test";
        DELETE remove() { format concord::Json; }
    }
}
concord::api! {
    pub client TwiceOnTheEndpoint {
        base "http://x.test";
        POST note(body note: String) {
            request concord::Text;
            request concord::Binary;
        }
    }
}
concord::api! {
    pub client FormatAndRequest {
        base "http://x.test";
        POST note(body note: String) -> String {
            format concord::Text;
            request concord::Text;
        }
    }
}
concord::api! {
    pub client RequestAndFormat {
        base "http://x.test";
        POST note(body note: String) -> String {
            request concord::Text;
            format concord::Text;
        }
    }
}
concord::api! {
    pub client Twice {
        base "http://x.test";
        response concord::Json;
        response concord::Text;
        GET version() -> String;
    }
}
concord::api! {
    pub client AfterChild {
        base "http://x.test";
        GET version() -> String;
        response concord::Json;
    }
}
// Accepted: `format` needs one of the two directions, not both.
concord::api! {
    pub client OnlyABody {
        base "http://x.test";
        POST note(body note: String) { format concord::Text; }
    }
}
concord::api! {
    pub client OnlyAnOutput {
        base "http://x.test";
        GET version() -> String { format concord::Text; }
    }
}
fn main() {}
