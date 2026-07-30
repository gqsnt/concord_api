use concord::api;

api! {
    client FlatSecretApi {
        base "https://example.test/";
        secret token: String;
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
        params { "id" = vars.id; }
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
        auth certificate;
    }
}

api! {
    client AuthCombinatorApi {
        base "https://example.test/";
        auth any;
    }
}

api! {
    client AllAuthCombinatorApi {
        base "https://example.test/";
        auth all;
    }
}

api! {
    client OneOfAuthCombinatorApi {
        base "https://example.test/";
        auth one_of;
    }
}

api! {
    client OptionalAuthCombinatorApi {
        base "https://example.test/";
        auth optional;
    }
}

api! {
    client InterpolationApi {
        base "https://example.test/";
    }

    GET read(id: String) -> Bytes {
        path [["items-", ep.id]];
    }
}

fn main() {}
