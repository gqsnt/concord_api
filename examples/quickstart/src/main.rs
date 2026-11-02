//! `cargo run -p concord-example-quickstart`: starts the local stand-in server and runs the quick
//! start against it.

use concord_example_quickstart::local_server::{LocalServer, tasks_service};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = "local-demo-token";
    // A stand-in for https://tasks.example.com on 127.0.0.1, so the example runs offline.
    let server = LocalServer::start(tasks_service(token)).await?;
    concord_example_quickstart::run(&format!("{}/api", server.url()), token.to_owned()).await
}
