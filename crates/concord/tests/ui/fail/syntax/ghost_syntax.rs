use concord::api;

api! {
    client FlatSecretApi {
        timeout off;
        base "https://example.test/";
        secret token;
    }
}

api! {
    client FlatCredentialApi {
        timeout off;
        base "https://example.test/";
        credential token = bearer(secret.token);
    }
}

api! {
    client DefaultBlockApi {
        timeout off;
        base "https://example.test/";
        default { timeout 1s; }
    }
}

api! {
    client DefaultsBlockApi {
        timeout off;
        base "https://example.test/";
        defaults { timeout 1s; }
    }
}

api! {
    client PoliciesBlockApi {
        timeout off;
        base "https://example.test/";
        policies { }
    }
}

api! {
    client BehaviorsBlockApi {
        timeout off;
        base "https://example.test/";
        behaviors { }
    }
}

api! {
    client BehaviorApi {
        timeout off;
        base "https://example.test/";
        behavior finite;
    }
}

api! {
    client PrefixApi {
        timeout off;
        base "https://example.test/";
        prefix ["v1"];
    }
}

api! {
    client ParamsApi {
        timeout off;
        base "https://example.test/";
        params { "id" = client.id; }
    }
}

api! {
    client ProfileApi {
        timeout off;
        base "https://example.test/";
        profile standard { }
    }
}

api! {
    client AccessTokenApi {
        timeout off;
        base "https://example.test/";
        access_token token: String;
    }
}

api! {
    client CertificateApi {
        timeout off;
        base "https://example.test/";
        auth { certificate; }
    }
}

api! {
    client AuthCombinatorApi {
        timeout off;
        base "https://example.test/";
        auth { any; }
    }
}

api! {
    client AllAuthCombinatorApi {
        timeout off;
        base "https://example.test/";
        auth { all; }
    }
}

api! {
    client OneOfAuthCombinatorApi {
        timeout off;
        base "https://example.test/";
        auth { one_of; }
    }
}

api! {
    client OptionalAuthCombinatorApi {
        timeout off;
        base "https://example.test/";
        auth { optional; }
    }
}

api! {
    client InterpolationApi {
        timeout off;
        base "https://example.test/";


    GET read(id: String) -> Bytes using Bytes {
        path [["items-", endpoint.id]];
    }
}}

fn main() {}
