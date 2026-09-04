use concord::{Bytes, Text, api};

const DEFAULT_COUNT: u64 = 25;

api! {
    /// Compile-checked v2 language surface.
    client V2LanguageApi {
        base "https://example.test/root/";
        timeout 20s;

        values {
            /// Required domain payload.
            tenant: String;
            locale?: String;
            #[default]
            retry_hint?: Option<u8>;
        }

        subdomain ["api"];
        path [client.tenant];
        query {
            "locale" = client.locale;
        }

        scope regional(region: String, language?: String) {
            subdomain [regional.region];
            headers {
                "X-Language" = regional.language;
            }

            GET list(
                required_option: Option<String>,
                tags: Vec<String>,
                count: u64 = DEFAULT_COUNT,
            ) -> Bytes via Binary {
                id "v2.language.list";
                path ["items"];
                query {
                    "tag" = each endpoint.tags;
                    "count" = endpoint.count;
                }
                effect idempotent;
                expose accepted_bytes;
            }

            POST update(
                id: String,
                mode?: Option<String>,
                body input: String via Text,
            ) -> String via Text {
                id "v2.language.update";
                path ["items", endpoint.id];
                effect non_idempotent;
            }
        }
    }
}

api! {
    client ReflectedAuthoritiesApi {
        base "https://example.test";
        timeout 30s;

        values {
            tenant: String;
        }

        credentials {
            secret token;
            credential bearer_auth = bearer(secret.token);
        }

        rate_limits {
            policy tenant_reads(tenant) {
                id "v2.reflection.tenant";
                key [origin, tenant];
                cost 2;
                allow 10 per 1m;
            }
        }

        GET fetch() -> Bytes via Binary {
            id "v2.reflection.fetch";
            auth {
                bearer bearer_auth;
            }
            rate_limit tenant_reads(tenant = client.tenant);
        }
    }
}

#[test]
fn exact_value_and_capability_surface_compiles() {
    let client = V2LanguageApi::builder()
        .set_tenant("tenant".to_owned())
        .with_locale("fr".to_owned())
        .omit_locale()
        .reset_retry_hint()
        .build()
        .expect("v2 client");

    let regional = client
        .regional("eu".to_owned())
        .with_language("French".to_owned());
    let list = regional
        .list(None, vec!["one".to_owned(), "two".to_owned()])
        .reset_count();
    let _: concord::endpoint::RequestPlan = list.plan().expect("plan");
    let _bytes = list.accepted_bytes();

    let intent = V2LanguageApi::unbound()
        .regional("eu".to_owned())
        .with_language("French".to_owned())
        .list(None, vec!["one".to_owned()]);
    assert_eq!(intent.description().id(), "v2.language.list");
    let _ = client.plan(&intent).expect("unbound plan");
    let _bound: v2_language_api::regional::ListRequest = client.request(intent);

    let _update = regional
        .update("id".to_owned(), "body".to_owned())
        .set_mode(Some(None))
        .with_mode(Some("replace".to_owned()))
        .omit_mode();
}

#[test]
fn contract_reflection_preserves_the_domain_tree_and_exact_semantics() {
    use concord::endpoint::{
        AttachmentFamily, AttachmentView, ObservationClass, ValueInitializer, ValueOwnerKind,
        ValuePresence,
    };

    let contract = V2LanguageApi::contract();
    assert_eq!(contract.declaration().name(), "V2LanguageApi");
    assert_eq!(
        contract.declaration().docs(),
        [" Compile-checked v2 language surface."]
    );
    assert_eq!(contract.base_origin(), "https://example.test/root/");
    assert_eq!(contract.values().len(), 3);
    assert_eq!(contract.values()[0].owner(), ValueOwnerKind::Client);
    assert_eq!(contract.values()[0].name(), "tenant");
    assert_eq!(contract.values()[0].presence(), ValuePresence::Required);
    assert_eq!(contract.values()[1].presence(), ValuePresence::Optional);
    assert_eq!(
        contract.values()[2].initializer(),
        ValueInitializer::TraitDefault
    );
    assert!(contract.values()[2].payload_type().contains("Option"));
    assert!(
        contract
            .effective_attachments()
            .iter()
            .all(|attachment| attachment.view() == AttachmentView::Effective)
    );

    let regional = &contract.scopes()[0];
    assert_eq!(regional.declaration().name(), "regional");
    assert_eq!(regional.values()[0].owner(), ValueOwnerKind::Scope);
    assert_eq!(regional.endpoints().len(), 2);
    assert!(regional.effective_attachments().iter().any(|attachment| {
        attachment.family() == AttachmentFamily::Subdomain && attachment.origin() == "client"
    }));
    assert!(regional.effective_attachments().iter().any(|attachment| {
        attachment.family() == AttachmentFamily::Subdomain
            && attachment.origin() == "scope regional"
    }));

    let list = regional
        .endpoints()
        .iter()
        .find(|endpoint| endpoint.id() == "v2.language.list")
        .expect("reflected list endpoint");
    let details = list.contract().expect("generated endpoint details");
    assert!(details.explicit_endpoint_id());
    assert_eq!(details.values()[0].name(), "required_option");
    assert_eq!(details.values()[0].presence(), ValuePresence::Required);
    assert_eq!(
        details.values()[0].observation(),
        ObservationClass::OrdinaryPublic
    );
    assert_eq!(details.response().output_type(), "Bytes");
    assert_eq!(details.response().specification_type(), "Binary");
    assert!(
        details
            .effective_attachments()
            .iter()
            .any(|attachment| attachment.family() == AttachmentFamily::Query
                && attachment.view() == AttachmentView::Effective
                && attachment.target() == Some("tag"))
    );
}

#[test]
fn contract_reflection_includes_value_free_authority_and_profile_schemas() {
    use concord::endpoint::{ContractFactClass, CredentialSourceKind, ObservationClass};

    let contract = ReflectedAuthoritiesApi::contract();
    assert_eq!(contract.secrets().len(), 1);
    assert_eq!(contract.secrets()[0].name(), "token");
    assert_eq!(
        contract.secrets()[0].observation(),
        ObservationClass::PrivateSecretOrCredential
    );

    assert_eq!(contract.credentials().len(), 1);
    let credential = &contract.credentials()[0];
    assert_eq!(credential.name(), "bearer_auth");
    assert_eq!(credential.source(), CredentialSourceKind::Bearer);
    assert_eq!(credential.protected_input_count(), 1);
    assert_eq!(
        credential.behavior_class(),
        ContractFactClass::StaticDeclaration
    );
    assert_eq!(credential.endpoint_id(), None);
    assert_eq!(credential.provider_type(), None);

    assert_eq!(contract.rate_profiles().len(), 1);
    let profile = &contract.rate_profiles()[0];
    assert_eq!(profile.name(), "tenant_reads");
    assert_eq!(profile.id(), "v2.reflection.tenant");
    assert!(profile.explicit_id());
    assert!(!profile.rename_sensitive());
    assert_eq!(profile.formals(), ["tenant"]);
    assert_eq!(profile.dimensions(), ["origin", "tenant"]);
    assert_eq!(profile.cost(), 2);
    assert_eq!(profile.windows().len(), 1);
}
