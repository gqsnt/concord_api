//! The S3 facade keeps its existing error variants and accessors.
use concord_sdk_amazon_s3::{
    BucketName, Credentials, ObjectKey, Region, S3Client, S3Error, http_client_builder,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let credentials = Credentials::new(
        std::env::var("AWS_ACCESS_KEY_ID")?,
        std::env::var("AWS_SECRET_ACCESS_KEY")?,
    )?;
    let s3 = S3Client::new(
        http_client_builder().build()?,
        Region::new("eu-west-3")?,
        credentials,
    )?;
    let bucket = s3.bucket(BucketName::new("my-bucket")?);
    match bucket
        .get_object(ObjectKey::new("missing.txt")?)
        .send()
        .await
    {
        Ok(output) => {
            let _ = output.bytes(1024).await?;
        }
        Err(S3Error::Api(api)) => {
            println!("HTTP {}; code {:?}", api.status(), api.code());
            // Parsed XML and raw bytes are explicit access, both already bounded.
            let _document = api.failure().document();
            let _bounded_bytes = api.failure().body();
        }
        Err(S3Error::NotModified(_)) => println!("the cached copy is current"),
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
