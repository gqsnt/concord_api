// Syntax that was never part of the selected grammar (or of any earlier
// Concord grammar) is rejected with a coded diagnostic instead of being
// silently discarded or reinterpreted (§3.2, §26.2 "no alternative dialects").
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client FlatSecretApi {
        base "https://example.test/";
        secret token;
    }
}

api! {
    client FlatCredentialApi {
        base "https://example.test/";
        credential token = bearer(secret.token);
    }
}

api! {
    client DefaultBlockApi {
        base "https://example.test/";
        default { timeout 1s; }
    }
}

api! {
    client DefaultsBlockApi {
        base "https://example.test/";
        defaults { timeout 1s; }
    }
}

api! {
    client PoliciesBlockApi {
        base "https://example.test/";
        policies { }
    }
}

api! {
    client BehaviorsBlockApi {
        base "https://example.test/";
        behaviors { }
    }
}

api! {
    client BehaviorApi {
        base "https://example.test/";
        behavior finite;
    }
}

api! {
    client PrefixApi {
        base "https://example.test/";
        prefix ["v1"];
    }
}

api! {
    client ParamsApi {
        base "https://example.test/";
        params { "id" = id; }
    }
}

api! {
    client ProfileApi {
        base "https://example.test/";
        profile standard { }
    }
}

api! {
    client AccessTokenApi {
        base "https://example.test/";
        access_token token: String;
    }
}

api! {
    client CertificateApi {
        base "https://example.test/";
        auth { certificate; }
    }
}

api! {
    client AuthCombinatorApi {
        base "https://example.test/";
        auth { any; }
    }
}

api! {
    client AllAuthCombinatorApi {
        base "https://example.test/";
        auth { all; }
    }
}

api! {
    client OneOfAuthCombinatorApi {
        base "https://example.test/";
        auth { one_of; }
    }
}

api! {
    client OptionalAuthCombinatorApi {
        base "https://example.test/";
        auth { optional; }
    }
}

// Bracketed interpolation is not an atom; `concat [...]` builds one scalar.
api! {
    client InterpolationApi {
        base "https://example.test/";

        GET read(id: String) -> Bytes via Binary {
            headers { "X-Item" = ["items-", id]; }
        }
    }
}

// A host template string is not an alternative DNS grammar (ORIGIN-02).
api! {
    client HostTemplateApi {
        base "https://example.test/";
        values { tenant: String; }
        subdomain "{tenant}.api";
    }
}

// A path hole names a declared value; it never evaluates an expression
// (PATH-01).
api! {
    client ExpressionHoleApi {
        base "https://example.test/";

        GET read(id: String) -> Bytes via Binary {
            path "items/{id.len()}";
        }
    }
}

fn main() {}
