use concord::{Text, api};

api! {
    client DefaultEffect {
        base "http://example.test/";
        POST update() -> String via Text {
            revalidate etag;
        }
    }
}

api! {
    client ExplicitEffect {
        base "http://example.test/";
        GET item() -> String via Text {
            effect idempotent;
            revalidate etag;
        }
    }
}

fn main() {}
