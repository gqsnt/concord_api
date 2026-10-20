// Credential suggestions use declared sources with the placement's capability.
#![allow(unused_imports)]

use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Binary, Bytes, api};

api! {
    client CredentialSpelling {
        base "https://example.test";
        credentials {
            secret token;
            credential session_token = bearer(secret.token);
        }
        GET read() -> Bytes via Binary {
            auth { bearer SessionToken; }
        }
    }
}

api! {
    client AmbiguousCredential {
        base "https://example.test";
        credentials {
            secret token;
            secret key;
            credential token = bearer(secret.token);
            credential taken = api_key(secret.key);
        }
        GET read() -> Bytes via Binary {
            auth { bearer tiken; }
        }
    }
}

api! {
    client BasicCandidates {
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            secret token;
            credential logon = bearer(secret.token);
            credential login = basic(secret.username, secret.password);
        }
        GET read() -> Bytes via Binary {
            auth { basic logn; }
        }
    }
}

api! {
    client BearerCandidates {
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            secret token;
            credential login = basic(secret.username, secret.password);
            credential logon = bearer(secret.token);
        }
        GET read() -> Bytes via Binary {
            auth { bearer logn; }
        }
    }
}

api! {
    client DistantCredential {
        base "https://example.test";
        credentials {
            secret token;
            credential session = bearer(secret.token);
        }
        GET read() -> Bytes via Binary {
            auth { bearer unrelated; }
        }
    }
}

api! {
    client CredentialSourcePath {
        base "https://example.test";
        credentials {
            credential session = endpoint accuonts::logni;
        }
        scope accounts() {
            POST login() -> Bytes via Binary;
        }
    }
}

api! {
    client CredentialSourceEndpoint {
        base "https://example.test";
        credentials {
            credential session = endpoint accounts::logn;
        }
        scope accounts() {
            scope logn() {}
            POST login() -> Bytes via Binary;
        }
    }
}

api! {
    client IncompatibleCredentialSource {
        base "https://example.test";
        credentials {
            credential session = endpoint auth::logn;
        }
        scope auth() {
            GET login() -> Option<Bytes> via Binary {
                absent on 404;
            }
            GET logon(query offset: u64 = 0) -> Bytes via Binary {
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: std::num::NonZeroU32::MIN };
                    cursor -> offset;
                }
            }
        }
    }
}

fn main() {}
