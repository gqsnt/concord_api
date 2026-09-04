use concord::api;

api! {
    client CallDefault {
        timeout 30s;
        base "https://example.test";
        values {
            value: String = String::new();
        }
    }
}

api! {
    client MethodDefault {
        timeout 30s;
        base "https://example.test";
        values {
            value: String = crate::TEXT.to_owned();
        }
    }
}

api! {
    client MacroDefault {
        timeout 30s;
        base "https://example.test";
        values {
            value: Vec<u8> = vec![];
        }
    }
}

api! {
    client BlockDefault {
        timeout 30s;
        base "https://example.test";
        values {
            value: u8 = { 0 };
        }
    }
}

api! {
    client CollectionDefault {
        timeout 30s;
        base "https://example.test";
        values {
            value: Vec<u8> = [0];
        }
    }
}

api! {
    client BodyDefault {
        timeout 30s;
        base "https://example.test";


    POST send(body payload: Bytes via Binary = default) -> Bytes via Binary;
}}

const TEXT: &str = "text";

fn main() {}
