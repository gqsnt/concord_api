// PATH-01 / V3-S05: generated path templates are relative, have no empty,
// `.`/`..` or backslash segments, balance their braces, name one declared
// total value per hole, and reject literal `?`/`#`. Diagnostics carry the
// whole literal span plus the decoded byte range (§5.3).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client LeadingSlashApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "/items"; }
    }
}

api! {
    client TrailingSlashApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items/"; }
    }
}

api! {
    client EmptySegmentApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items//detail"; }
    }
}

api! {
    client DotSegmentApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items/./detail"; }
    }
}

api! {
    client BackslashSegmentApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items\\detail"; }
    }
}

api! {
    client UnmatchedCloseApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items}"; }
    }
}

api! {
    client UnterminatedHoleApi {
        base "https://path.example.test/";

        GET read(id: String) -> Bytes via Binary { path "items/{id"; }
    }
}

api! {
    client EmptyHoleApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items/{}"; }
    }
}

api! {
    client OptionalHoleApi {
        base "https://path.example.test/";

        GET read(id?: String) -> Bytes via Binary { path "items/{id}"; }
    }
}

api! {
    client QueryMarkApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items?view=full"; }
    }
}

api! {
    client FragmentMarkApi {
        base "https://path.example.test/";

        GET read() -> Bytes via Binary { path "items#top"; }
    }
}

api! {
    client BracedAdapterApi {
        base "https://path.example.test/";

        GET read(id: String) -> Bytes via Binary { path "items/{id via Pad<{ 4 }>}"; }
    }
}

api! {
    client TwoHoleReferenceApi {
        base "https://path.example.test/";

        GET read(id: String, other: String) -> Bytes via Binary { path "items/{id other}"; }
    }
}

api! {
    client EachHoleApi {
        base "https://path.example.test/";

        GET read(ids: Vec<String>) -> Bytes via Binary { path "items/{each ids}"; }
    }
}

// `path replace`/`path off` follow the same template rules.
api! {
    client ReplaceTemplateApi {
        base "https://path.example.test/";

        scope s() {
            path replace "/root";
            GET read() -> Bytes via Binary;
        }
    }
}

// A path atom list is retired v2 syntax, reported with its template spelling.
api! {
    client PathListApi {
        base "https://path.example.test/";

        GET read(id: String) -> Bytes via Binary { path ["items", id]; }
    }
}

fn main() {}
