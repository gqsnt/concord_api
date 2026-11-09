//! The SDK error variants a caller matches, after `derive(concord::Error)` (needs `SF_INSTANCE_URL`, `SF_ACCESS_TOKEN`).

use std::time::Duration;

use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl};
use concord_example_salesforce_sdk::model::SObjectDescribe;
use concord_example_salesforce_sdk::{OrgContext, SfError};

async fn describe(context: &OrgContext) -> Result<SObjectDescribe, SfError> {
    // The derive supplies the same From<concord::Error> used by this `?`.
    let prepared = context.api().accounts().describe().prepare()?;
    match prepared.send().await {
        Ok(document) => Ok(document),
        // A2 does not read or classify a response during From conversion.
        Err(error) => Err(SfError::from_concord(error).await),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = concord::reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .build()?;
    let context = OrgContext::new(
        http,
        InstanceUrl::parse(&std::env::var("SF_INSTANCE_URL")?)?,
        ApiVersion::PINNED,
        AccessToken::new(std::env::var("SF_ACCESS_TOKEN")?)?,
    )?;

    match describe(&context).await {
        Ok(_) => println!("Account describe received"),
        Err(error) => {
            match &error {
                SfError::Api(failure) => {
                    eprintln!("HTTP {}; body {:?}", failure.status(), failure.body_state());
                    if failure.has_error_code("INVALID_SESSION_ID") {
                        eprintln!("The caller must obtain a new session before another call");
                    }
                }
                SfError::Concord(cause) => eprintln!("Concord {:?}", cause.kind()),
                _ => eprintln!("{error}"),
            }
            return Err(error.into());
        }
    }
    Ok(())
}
