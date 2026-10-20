#![allow(unused_imports)]

use concord::{api, Empty};

api! {
    client ClientLiteral {
        base "http://example.test/";
        headers { "X-Test" = "bad\rvalue"; }
        
    }
}

api! {
    client ScopeLiteral {
        base "http://example.test/";
        
        scope nested() { headers replace { "X-Test" = '\0'; } }
    }
}

api! {
    client EndpointConcat {
        base "http://example.test/";
        
        GET fetch(value: String) -> () via Empty { headers { append "X-Test" = concat [value, concat ["ok", "bad\n"]]; } }
    }
}

api! {
    client ReplaceLiteral {
        base "http://example.test/";
        
        GET fetch() -> () via Empty { headers { replace "X-Test" = '\u{7f}'; } }
    }
}

api! {
    client WhenPresentLiteral {
        base "http://example.test/";
        
        GET fetch() -> () via Empty { headers { when_present "X-Test" = "bad\u{1f}"; } }
    }
}

fn main() {}
