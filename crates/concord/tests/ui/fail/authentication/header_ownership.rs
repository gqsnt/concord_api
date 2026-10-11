// HEAD-01 / V3-S08: public, private-auth, body-media, negotiation and
// transport targets have distinct owners. Potential collisions are rejected
// at compile time before presence is known: an absent optional public writer
// does not make an overlapping private-auth placement legal.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

// Optional public header writer vs. an inherited private-auth header target.
api! {
    client OptionalPublicHeaderApi {
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        auth { header "X-Api-Key" = key; }

        GET read(header("x-api-key") public_key?: String) -> Bytes via Binary;
    }
}

// Public query writer declared first, private-auth query target later.
api! {
    client PublicQueryApi {
        base "https://example.test/";
        values { query api_key?: String; }
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }

        GET read() -> Bytes via Binary {
            auth { query "api_key" = key; }
        }
    }
}

// Body media and negotiation fields are not public header targets.
api! {
    client ContentTypeHeaderApi {
        base "https://example.test/";

        POST write(body payload: Bytes via Binary) -> Bytes via Binary {
            headers { "Content-Type" = "text/plain"; }
        }
    }
}

api! {
    client AcceptPlacementApi {
        base "https://example.test/";

        GET read(header("Accept") accept?: String) -> Bytes via Binary;
    }
}

// Transport framing fields are not public header targets.
api! {
    client TransferEncodingApi {
        base "https://example.test/";
        headers { "transfer-encoding" = "chunked"; }
    }
}

api! {
    client HostHeaderApi {
        base "https://example.test/";

        GET read() -> Bytes via Binary {
            headers { "Host" = "other.example.test"; }
        }
    }
}

// Singleton request fields cannot be repeated with `each` or `append`.
api! {
    client SingletonEachApi {
        base "https://example.test/";

        GET read(agents: Vec<String>) -> Bytes via Binary {
            headers { "User-Agent" = each agents; }
        }
    }
}

api! {
    client SingletonAppendApi {
        base "https://example.test/";
        headers { "Range" = "bytes=0-1"; }

        GET read() -> Bytes via Binary {
            headers { append "range" = "bytes=2-3"; }
        }
    }
}

// Header names are validated HTTP field-name tokens of at most 256 bytes.
api! {
    client InvalidHeaderNameApi {
        base "https://example.test/";
        headers { "X Bad" = "1"; }
    }
}

fn main() {}
