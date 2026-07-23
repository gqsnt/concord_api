#![forbid(unsafe_code)]

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    concord_examples::riot::usage::run_live_from_environment().await
}
