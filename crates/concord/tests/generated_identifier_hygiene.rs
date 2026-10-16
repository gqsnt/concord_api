//! Authored constants keep their lexical meaning beside private bindings.

#[allow(non_upper_case_globals, dead_code)]
mod declarations {
    use concord::auth::{AccessToken, AuthError, BoxFuture, CredentialProvider, CredentialRequest};
    use concord::{Empty, Text, api};

    const v_input: u32 = 7;
    const i_retained: u32 = 11;
    const value: u32 = 17;

    // These ordinary items must not become patterns or references in generated
    // support code. Deliberate use of the reserved __concord namespace is not
    // part of the authored language contract.
    macro_rules! unrelated_constants {
        ($($name:ident),* $(,)?) => { $(const $name: u32 = 23;)* };
    }
    unrelated_constants!(
        values,
        local,
        builder,
        writer,
        sink,
        emission,
        auth,
        formatter,
        client,
        endpoint,
        material,
        provider,
        error,
        core,
        update,
        configure,
        http,
        runtime,
        policy,
        missing,
        cursor,
        init,
        segments,
        inherited,
        visit,
        spec,
        token,
        options,
        owner,
        view,
        maximum_pages,
        credential,
        duration,
        s_bootstrap,
        c_access,
        c_delegated,
        p_delegated,
        __body,
        __cursor_0,
    );

    pub type Input = u32;
    pub type Payload = String;
    const PAGE_SIZE: std::num::NonZeroU32 = std::num::NonZeroU32::new(10).unwrap();

    api! {
        client IdentifierHygieneApi {
            base "https://hygiene.example.test/";
            values { query root: Input = value; }

            GET item(query input: Input, query retained: Input = i_retained) -> () via Empty;

            scope nested(
                query("scopeInput") input: Input,
                query("scopeRetained") retained: Input = i_retained,
            ) {
                GET item(query input: Input, query retained: Input = v_input) -> () via Empty;
                // The credential facade exists only in the client module.
                // A nested scope may use this handle name in its own module.
                scope credentials() {
                    GET read() -> () via Empty;
                }
            }
        }
    }

    pub struct Provider;

    impl CredentialProvider for Provider {
        type Material = AccessToken;

        fn acquire<'a>(
            &'a self,
            _request: CredentialRequest<'a>,
        ) -> BoxFuture<'a, Result<AccessToken, AuthError>> {
            panic!("inert construction and planning must not acquire a credential")
        }
    }

    api! {
        client CredentialBodyHygieneApi {
            base "https://hygiene.example.test/";
            credentials {
                secret bootstrap;
                credential access = api_key(secret.bootstrap);
                credential delegated = custom Provider;
            }
            auth {
                header "X-Key" = access;
                bearer delegated;
            }
            POST put(id: Input, body payload: Payload via Text) -> () via Empty {
                path "items/item-{id}";
                effect idempotent;
            }
            PUT patch(body payload?: Payload via Text) -> () via Empty {
                path "items";
                effect idempotent;
            }
        }
    }

    #[cfg(feature = "json")]
    api! {
        client PaginationHygieneApi {
            base "https://hygiene.example.test/";
            GET list(query offset: u64 = 0, query count: std::num::NonZeroU32 = PAGE_SIZE) -> Vec<u32> via concord::Json {
                paginate via concord::pagination::OffsetPagination {
                    init concord::pagination::OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }
        }
    }
}

use declarations::{CredentialBodyHygieneApi, IdentifierHygieneApi, Provider};

#[test]
fn generated_private_bindings_preserve_authored_constants_and_input_values() {
    let client = IdentifierHygieneApi::new().expect("client");
    let direct = client.item(3).plan().expect("direct plan");
    assert_eq!(
        direct.observed_url(),
        "https://hygiene.example.test/?root=17&input=3&retained=11"
    );

    let direct = client.nested(5).item(9).plan().expect("nested plan");
    let unbound = IdentifierHygieneApi::unbound().nested(5).item(9);
    let bound = client.request(unbound).plan().expect("bound plan");
    assert_eq!(
        direct.observed_url(),
        "https://hygiene.example.test/?root=17&scopeInput=5&scopeRetained=11&input=9&retained=7"
    );
    assert_eq!(direct.observed_url(), bound.observed_url());

    let reset = client
        .nested(5)
        .set_retained(100)
        .reset_retained()
        .item(9)
        .set_retained(200)
        .reset_retained()
        .plan()
        .expect("retained initial values");
    assert_eq!(direct.observed_url(), reset.observed_url());
    assert_eq!(
        client
            .nested(5)
            .credentials()
            .read()
            .plan()
            .unwrap()
            .observed_url(),
        "https://hygiene.example.test/?root=17&scopeInput=5&scopeRetained=11"
    );
}

#[test]
fn credential_and_body_storage_use_private_bindings() {
    let client = CredentialBodyHygieneApi::builder()
        .bootstrap(concord::SecretString::try_new("test-key").unwrap())
        .delegated(Provider)
        .build()
        .expect("client with a provider and secret");
    let direct = client.put(31, "payload".to_owned()).plan().unwrap();
    let unbound = CredentialBodyHygieneApi::unbound().put(31, "payload".to_owned());
    assert_eq!(
        direct.observed_url(),
        "https://hygiene.example.test/items/item-31"
    );
    assert_eq!(
        direct.observed_url(),
        client.plan(&unbound).unwrap().observed_url()
    );
    assert_eq!(
        client
            .patch()
            .set_payload("patch".to_owned())
            .omit_payload()
            .plan()
            .unwrap()
            .observed_url(),
        "https://hygiene.example.test/items"
    );
    client.credentials().clear_access().unwrap();
    client.credentials().clear_delegated().unwrap();
}

#[cfg(feature = "json")]
#[test]
fn pagination_bindings_preserve_controller_types_and_defaults() {
    let client = declarations::PaginationHygieneApi::new().unwrap();
    assert_eq!(
        client.list().plan().unwrap().observed_url(),
        "https://hygiene.example.test/?offset=0&count=10"
    );
    let _traversal = client.list().pages(1);
}
