// Representation statements on an endpoint need something to apply to: `request` needs a body,
// `response` a typed output (`-> T`), `format` at least one of the two. An owner declares each
// direction at most once (`format` counts as both) and its statements precede its children.
concord::api! {
    pub client RequestWithoutBody {
        base "http://x.test";
        GET version() -> String { request text; response text; }
    }
}
concord::api! {
    pub client ResponseWithoutOutput {
        base "http://x.test";
        POST note(body note: String) { request text; response text; }
    }
}
concord::api! {
    pub client FormatWithNothing {
        base "http://x.test";
        DELETE remove() { format json; }
    }
}
concord::api! {
    pub client TwiceOnTheEndpoint {
        base "http://x.test";
        POST note(body note: String) {
            request text;
            request bytes;
        }
    }
}
concord::api! {
    pub client FormatAndRequest {
        base "http://x.test";
        POST note(body note: String) -> String {
            format text;
            request text;
        }
    }
}
concord::api! {
    pub client RequestAndFormat {
        base "http://x.test";
        POST note(body note: String) -> String {
            request text;
            format text;
        }
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
        GET version() -> String;
        response json;
    }
}
// Accepted: `format` needs one of the two directions, not both.
concord::api! {
    pub client OnlyABody {
        base "http://x.test";
        POST note(body note: String) { format text; }
    }
}
concord::api! {
    pub client OnlyAnOutput {
        base "http://x.test";
        GET version() -> String { format text; }
    }
}
fn main() {}
