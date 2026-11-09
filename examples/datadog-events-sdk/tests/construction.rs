//! Direct Config literals require checked keys, just like the workflow constructor.
use concord_sdk_datadog_events::{
    ApiKey, ApplicationKey, AuthError, Credentials, DatadogApi, DatadogApiConfig, Site,
};

#[test]
fn public_key_constructors_check_every_input() {
    for value in ["", " ", "bad\nkey", "bad\rkey", "bad\0key"] {
        assert!(ApiKey::new(value).is_err());
        assert!(ApplicationKey::new(value).is_err());
        assert!(Credentials::new(value, "app").is_err());
        assert!(Credentials::new("api", value).is_err());
    }
    assert!(matches!(ApiKey::new(""), Err(AuthError::MissingApiKey)));
    assert!(matches!(
        ApplicationKey::new(""),
        Err(AuthError::MissingApplicationKey)
    ));
    let config = DatadogApiConfig {
        destination: Site::Eu.into(),
        api_key: ApiKey::new("CONSTRUCTION_API_CANARY").unwrap(),
        application_key: ApplicationKey::new("CONSTRUCTION_APP_CANARY").unwrap(),
    };
    let api = DatadogApi::new(concord::reqwest::Client::new(), config);
    let prepared = api.events().list().prepare().unwrap();
    assert_eq!(
        prepared.request().headers()["dd-api-key"],
        "CONSTRUCTION_API_CANARY"
    );
    assert_eq!(
        prepared.request().headers()["dd-application-key"],
        "CONSTRUCTION_APP_CANARY"
    );
    assert!(!format!("{api:?} {prepared:?}").contains("CANARY"));
}
