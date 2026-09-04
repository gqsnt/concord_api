use concord::api;

api! {
    client FlatSecretApi {
        timeout 30s;
        base "https://example.test/";
        secret token;
    }
}

api! {
    client FlatCredentialApi {
        timeout 30s;
        base "https://example.test/";
        credential token = bearer(secret.token);
    }
}

api! {
    client DefaultBlockApi {
        timeout 30s;
        base "https://example.test/";
        default { timeout 1s; }
    }
}

api! {
    client DefaultsBlockApi {
        timeout 30s;
        base "https://example.test/";
        defaults { timeout 1s; }
    }
}

api! {
    client PoliciesBlockApi {
        timeout 30s;
        base "https://example.test/";
        policies { }
    }
}

api! {
    client BehaviorsBlockApi {
        timeout 30s;
        base "https://example.test/";
        behaviors { }
    }
}

api! {
    client BehaviorApi {
        timeout 30s;
        base "https://example.test/";
        behavior finite;
    }
}

api! {
    client PrefixApi {
        timeout 30s;
        base "https://example.test/";
        prefix ["v1"];
    }
}

api! {
    client ParamsApi {
        timeout 30s;
        base "https://example.test/";
        params { "id" = client.id; }
    }
}

api! {
    client ProfileApi {
        timeout 30s;
        base "https://example.test/";
        profile standard { }
    }
}

api! {
    client AccessTokenApi {
        timeout 30s;
        base "https://example.test/";
        access_token token: String;
    }
}

api! {
    client CertificateApi {
        timeout 30s;
        base "https://example.test/";
        auth { certificate; }
    }
}

api! {
    client AuthCombinatorApi {
        timeout 30s;
        base "https://example.test/";
        auth { any; }
    }
}

api! {
    client AllAuthCombinatorApi {
        timeout 30s;
        base "https://example.test/";
        auth { all; }
    }
}

api! {
    client OneOfAuthCombinatorApi {
        timeout 30s;
        base "https://example.test/";
        auth { one_of; }
    }
}

api! {
    client OptionalAuthCombinatorApi {
        timeout 30s;
        base "https://example.test/";
        auth { optional; }
    }
}

api! {
    client InterpolationApi {
        timeout 30s;
        base "https://example.test/";


    GET read(id: String) -> Bytes via Binary {
        path [["items-", endpoint.id]];
    }
}}

fn main() {}
