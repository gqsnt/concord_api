//! Signing still runs in the SDK; its Decoder::execute emits the one head event.

use concord_sdk_amazon_s3::{
    BucketName, Credentials, ObjectKey, Region, S3Client, http_client_builder,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::INFO)
        .without_time()
        .with_ansi(false)
        .init();
    let mut credentials = Credentials::new(
        std::env::var("AWS_ACCESS_KEY_ID")?,
        std::env::var("AWS_SECRET_ACCESS_KEY")?,
    )?;
    if let Ok(session) = std::env::var("AWS_SESSION_TOKEN") {
        credentials = credentials.with_session_token(session)?;
    }
    let s3 = S3Client::new(
        http_client_builder().build()?,
        Region::new("eu-west-3")?,
        credentials,
    )?;
    let bucket = s3.bucket(BucketName::new(std::env::var("S3_BUCKET")?)?);
    let _head = bucket
        .head_object(ObjectKey::new(std::env::var("S3_KEY")?)?)
        .send()
        .await?;
    Ok(())
}
