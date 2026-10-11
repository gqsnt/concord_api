// §11.1 / C3-AUTH: the credential graph is resolved as a whole. Missing
// source paths, cycles, unknown credentials/secrets and source endpoints that
// cannot return complete material are rejected at the declaration.
#![allow(unused_imports)]

use concord::{Binary, Bytes, SecretString, Text, api};

api! {
    client MissingSourceApi {
        base "https://example.test";
        credentials {
            credential session = endpoint accounts::missing;
        }

        scope accounts() {
            POST login() -> Bytes via Binary;
        }
        GET read() -> Bytes via Binary {
            auth { bearer session; }
        }
    }
}

api! {
    client MissingScopeSourceApi {
        base "https://example.test";
        credentials {
            credential session = endpoint nowhere::login;
        }

        GET read() -> Bytes via Binary {
            auth { bearer session; }
        }
    }
}

api! {
    client CyclicSourceApi {
        base "https://example.test";
        credentials {
            credential session = endpoint login;
        }
        auth { bearer session; }

        POST login() -> Bytes via Binary;
    }
}

api! {
    client UnknownCredentialApi {
        base "https://example.test";

        GET read() -> Bytes via Binary {
            auth { bearer missing; }
        }
    }
}

api! {
    client UnknownSecretApi {
        base "https://example.test";
        credentials {
            credential key = api_key(secret.missing);
        }
    }
}

api! {
    client AbsentSourceApi {
        base "https://example.test";
        credentials {
            credential session = endpoint login;
        }

        GET login() -> Option<String> via Text {
            absent on 404;
        }
        GET read() -> Bytes via Binary {
            auth { authorization "Session" = session; }
        }
    }
}

fn main() {}
