// HEAD-01 / V3-S08 (static negative through the real expansion): potential
// ownership is decided before presence is known. A public writer whose source
// is outer-optional (absent-capable) is rejected on body-media, negotiation
// and transport fields exactly as a literal writer is, and an optional writer
// that is the only potential writer of a private-auth target still collides.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

// Body media: an optional direct placement beside a body.
api! {
    client OptionalContentTypeApi {
        base "https://example.test/";

        POST write(
            header("Content-Type") media?: String,
            body payload: Bytes via Binary,
        ) -> Bytes via Binary;
    }
}

// Body media: an optional appended length.
api! {
    client OptionalContentLengthApi {
        base "https://example.test/";

        POST write(length?: u64, body payload: Bytes via Binary) -> Bytes via Binary {
            headers { append "content-length" = length; }
        }
    }
}

// Negotiation: an optional repeated sequence.
api! {
    client OptionalAcceptEncodingApi {
        base "https://example.test/";

        GET read(encodings?: Vec<String>) -> Bytes via Binary {
            headers { "Accept-Encoding" = each encodings; }
        }
    }
}

// Transport: an optional scope value placed directly on `Host`.
api! {
    client OptionalHostApi {
        base "https://example.test/";

        scope tenant(header("Host") host?: String) {
            GET read() -> Bytes via Binary;
        }
    }
}

// Transport: a conditional override with an optional source.
api! {
    client OptionalTransferEncodingApi {
        base "https://example.test/";
        values { coding?: String; }
        headers { when_present "Transfer-Encoding" = coding; }
    }
}

// Private auth: the inherited literal writer is removed, so the only potential
// writer of the private target is an optional append.
api! {
    client OptionalAppendAuthApi {
        base "https://example.test/";
        credentials {
            secret key;
            credential key = api_key(secret.key);
        }
        headers { "X-Api-Key" = "public-fallback"; }

        GET read(hint?: String) -> Bytes via Binary {
            headers { remove "X-Api-Key"; append "x-api-key" = hint; }
            auth { header "X-Api-Key" = key; }
        }
    }
}

fn main() {}
