//! Every public construction route carries the checked request version.
use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl, SupportedVersion};
use concord_example_salesforce_sdk::{OrgContext, SalesforceApi, SalesforceApiConfig};

#[test]
fn public_construction_checks_the_minimum_version() {
    for version in [ApiVersion::new(40, 0), ApiVersion::new(57, 99)] {
        assert!(SupportedVersion::new(version).is_err());
        assert!(
            OrgContext::new(
                concord::reqwest::Client::new(),
                InstanceUrl::parse("https://acme.my.salesforce.com").unwrap(),
                version,
                AccessToken::new("synthetic-token").unwrap()
            )
            .is_err()
        );
    }
    for version in [ApiVersion::MINIMUM, ApiVersion::PINNED] {
        let config = SalesforceApiConfig {
            instance: InstanceUrl::parse("http://127.0.0.1:1234").unwrap(),
            token: "synthetic-token".into(),
            version: SupportedVersion::new(version).unwrap(),
        };
        let api = SalesforceApi::new(concord::reqwest::Client::new(), config);
        let request = api.accounts().describe().prepare().unwrap();
        assert_eq!(
            request.request().url().path(),
            format!("/services/data/v{version}/sobjects/Account/describe/")
        );
        assert_eq!(api.version().version(), version);
    }
}
