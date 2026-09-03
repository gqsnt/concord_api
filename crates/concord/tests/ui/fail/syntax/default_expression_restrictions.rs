use concord::api;

api! {
    client CallDefault {
        timeout off;
        base "https://example.test";
        values {
            value: String = String::new();
        }
    }
}

api! {
    client MethodDefault {
        timeout off;
        base "https://example.test";
        values {
            value: String = crate::TEXT.to_owned();
        }
    }
}

api! {
    client MacroDefault {
        timeout off;
        base "https://example.test";
        values {
            value: Vec<u8> = vec![];
        }
    }
}

api! {
    client BlockDefault {
        timeout off;
        base "https://example.test";
        values {
            value: u8 = { 0 };
        }
    }
}

api! {
    client CollectionDefault {
        timeout off;
        base "https://example.test";
        values {
            value: Vec<u8> = [0];
        }
    }
}

api! {
    client BodyDefault {
        timeout off;
        base "https://example.test";


    POST send(body payload: Bytes using Bytes = default) -> Bytes using Bytes;
}}

const TEXT: &str = "text";

fn main() {}
