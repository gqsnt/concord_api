// Header literals are checked in every owner and patch operation, including
// individual nested concat components adjacent to dynamic references.
#![allow(unused_imports)]
use concord::{Binary, Bytes, api};

api! {
    client ClientHeaderLiteral {
        base "http://example.test/";
        headers { "x-value" = "line\nbreak"; }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client ScopeHeaderLiteral {
        base "http://example.test/";
        headers { "x-value" = "valid"; }
        scope nested() {
            headers { replace "x-value" = '\r'; }
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client EndpointHeaderLiteral {
        base "http://example.test/";
        headers { "x-value" = "valid"; }
        GET read() -> Bytes via Binary {
            headers { append "x-value" = "nul\0byte"; }
        }
    }
}

api! {
    client ClientAppendHeaderLiteral {
        base "http://example.test/";
        headers { append "x-value" = '\u{7f}'; }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client ScopeIntroduceHeaderLiteral {
        base "http://example.test/";
        scope nested() {
            headers { "x-value" = '\u{1f}'; }
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client EndpointConcatHeaderLiteral {
        base "http://example.test/";
        headers { "x-value" = "valid"; }
        GET read(value: String) -> Bytes via Binary {
            headers {
                replace "x-value" = concat ["prefix", value, concat ['\n', "suffix"]];
            }
        }
    }
}

api! {
    client ScopeReplaceBlockHeaderLiteral {
        base "http://example.test/";
        headers { "x-value" = "valid"; }
        scope nested() {
            headers replace { "x-new" = "bad\u{b}value"; }
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client EndpointReplaceBlockHeaderLiteral {
        base "http://example.test/";
        GET read(value: String) -> Bytes via Binary {
            headers replace {
                "x-new" = concat [concat ["bad\rvalue", value], '-'];
            }
        }
    }
}

api! {
    client ConditionalHeaderLiteral {
        base "http://example.test/";
        headers { "x-value" = "valid"; }
        GET read() -> Bytes via Binary {
            headers { when_present "x-value" = "bad\nvalue"; }
        }
    }
}

fn main() {}
