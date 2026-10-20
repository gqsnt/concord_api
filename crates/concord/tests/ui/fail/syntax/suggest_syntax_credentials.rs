use concord::api;

api! { client SecretKeyword { base "https://example.test/"; credentials { secrte token; } } }
api! { client CredentialKeyword { base "https://example.test/"; credentials { credentail token = custom Provider; } } }
api! { client Source { base "https://example.test/"; credentials { secret token; credential key = apiKey(secret.token); } } }
api! { client SecretReference { base "https://example.test/"; credentials { secret token; credential key = bearer(secert.token); } } }
api! { client SecretName { base "https://example.test/"; credentials { secret token; credential key = bearer(secret.tokne); } } }
api! { client AuthModifier { base "https://example.test/"; auth replcae {} } }
api! { client AuthPlacement { base "https://example.test/"; auth { beareer key; } } }
api! { client AuthChallenge { base "https://example.test/"; auth { bearer key challenge unauthorizedOrForbidden; } } }
api! { client ChallengeKeyword { base "https://example.test/"; auth { bearer key challange never; } } }

fn main() {}
