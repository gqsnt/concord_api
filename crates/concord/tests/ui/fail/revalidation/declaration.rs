use concord::{Text, api};

api! {
    client ClientClause {
        base "http://example.test/";
        revalidate etag;
    }
}

api! {
    client ScopeClause {
        base "http://example.test/";
        scope resources() {
            revalidate etag;
        }
    }
}

api! {
    client DuplicateClause {
        base "http://example.test/";
        GET item() -> String via Text {
            revalidate etag;
            revalidate etag;
        }
    }
}

api! {
    client UnknownMode {
        base "http://example.test/";
        GET item() -> String via Text {
            revalidate last_modified;
        }
    }
}

api! {
    client MissingTerminator {
        base "http://example.test/";
        GET item() -> String via Text {
            revalidate etag
        }
    }
}

fn main() {}
