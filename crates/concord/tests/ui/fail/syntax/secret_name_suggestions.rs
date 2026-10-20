// Only earlier secrets are candidates; tied names are sorted, including Unicode.
use concord::api;

api! {
    client SecretSuggestionApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = api_key(secret.toekn);
        }
    }
}

api! {
    client SecretTieSuggestionApi {
        base "https://example.test/";
        credentials {
            secret cut;
            secret cat;
            credential key = api_key(secret.cot);
        }
    }
}

api! {
    client SecretQualifierSuggestionApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = api_key(secrt.token);
        }
    }
}

api! {
    client UnicodeSecretSuggestionApi {
        base "https://example.test/";
        credentials {
            secret clé;
            credential key = api_key(secret.clè);
        }
    }
}

api! {
    client RawSecretSuggestionApi {
        base "https://example.test/";
        credentials {
            secret r#type;
            credential key = api_key(secret.typo);
        }
    }
}

api! {
    client LaterSecretSuggestionApi {
        base "https://example.test/";
        credentials {
            credential key = api_key(secret.toekn);
            secret token;
        }
    }
}

api! {
    client DistantSecretSuggestionApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = api_key(secret.unrelated);
        }
    }
}

// Basic username/password secrets must remain distinct after correction.
api! {
    client BasicUsernameCounterpartExcludedApi {
        base "https://example.test/";
        credentials {
            secret username;
            credential key = basic(secret.usernmae, secret.username);
        }
    }
}

api! {
    client BasicPasswordCounterpartExcludedApi {
        base "https://example.test/";
        credentials {
            secret username;
            credential key = basic(secret.username, secret.usernmae);
        }
    }
}

api! {
    client BasicUsernameAlternativeApi {
        base "https://example.test/";
        credentials {
            secret cut;
            secret cat;
            credential key = basic(secret.cot, secret.cat);
        }
    }
}

api! {
    client BasicPasswordAlternativeApi {
        base "https://example.test/";
        credentials {
            secret cut;
            secret cat;
            credential key = basic(secret.cat, secret.cot);
        }
    }
}

fn main() {}
