//! Provider documents are typed; challenge/replay remains the SDK's behavior.
use concord_sdk_distribution_registry::{
    ErrorCode, ManifestMediaType, Reference, Registry, RegistryError, RepositoryName,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = concord::reqwest::Client::builder()
        .redirect(concord::reqwest::redirect::Policy::none())
        .build()?;
    let registry = Registry::new(http, "https://registry.example.test")?;
    let repository = registry.repository(&RepositoryName::new("acme/app")?)?;
    match repository
        .manifest(&Reference::tag("missing")?, &[ManifestMediaType::OciV1])
        .await
    {
        Ok(manifest) => println!("{} bytes", manifest.bytes.len()),
        Err(RegistryError::Api(api)) => {
            println!("HTTP {}", api.failure.status());
            if api.has_code(&ErrorCode::ManifestUnknown) {
                println!("the requested manifest is absent");
            }
            // Explicit, unredacted data is available without another body read.
            let _document = api.failure.document();
            let _bounded_bytes = api.failure.body();
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
