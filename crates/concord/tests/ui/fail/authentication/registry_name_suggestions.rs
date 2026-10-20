// Suggestions stay within declared secrets and compatible credentials.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client CredentialTypoApi {
        base "https://example.test";
        credentials {
            secret token;
            credential session = bearer(secret.token);
        }
        GET read() -> Bytes via Binary {
            auth { bearer sesison; }
        }
    }
}

api! {
    client BasicCredentialTypoApi {
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            credential login = basic(secret.username, secret.password);
        }
        GET read() -> Bytes via Binary {
            auth { basic logni; }
        }
    }
}

api! {
    client CredentialUnrelatedApi {
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
    client ApiKeySecretTypoApi {
        base "https://example.test";
        credentials {
            secret token;
            credential key = api_key(secret.toekn);
        }
    }
}

api! {
    client BearerSecretTypoApi {
        base "https://example.test";
        credentials {
            secret token;
            credential session = bearer(secret.tken);
        }
    }
}

api! {
    client BasicSecretTypoApi {
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            credential login = basic(secret.username, secret.passwrod);
        }
    }
}

api! {
    client SecretRoleApi {
        base "https://example.test";
        credentials {
            secret material;
            credential session = bearer(secret.material);
            credential key = api_key(secret.sesion);
        }
    }
}

api! {
    client SecretUnrelatedApi {
        base "https://example.test";
        credentials {
            secret token;
            credential session = bearer(secret.unrelated);
        }
    }
}

api! {
    client CredentialRoleApi {
        base "https://example.test";
        credentials {
            secret session;
            credential key = bearer(secret.session);
        }
        GET read() -> Bytes via Binary {
            auth { bearer sesison; }
        }
    }
}

api! {
    client BasicCredentialRoleApi {
        base "https://example.test";
        credentials {
            secret username;
            secret password;
            credential login = basic(secret.username, secret.password);
        }
        GET read() -> Bytes via Binary {
            auth { bearer logni; }
        }
    }
}

api! {
    client BearerCredentialRoleApi {
        base "https://example.test";
        credentials {
            secret token;
            credential session = bearer(secret.token);
        }
        GET read() -> Bytes via Binary {
            auth { basic sesison; }
        }
    }
}

api! {
    client RawCredentialTypoApi {
        base "https://example.test";
        credentials {
            secret token;
            credential r#type = bearer(secret.token);
        }
        GET read() -> Bytes via Binary {
            auth { bearer tpye; }
        }
    }
}

api! {
    client RawSecretTypoApi {
        base "https://example.test";
        credentials {
            secret r#type;
            credential session = bearer(secret.tpye);
        }
    }
}

fn main() {}
