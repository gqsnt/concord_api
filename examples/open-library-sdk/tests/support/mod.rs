use std::time::Duration;

use concord_example_testserver::TestServer;
use concord_sdk_open_library::{OpenLibrary, concord::reqwest};

pub fn client(server: &TestServer) -> OpenLibrary {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    OpenLibrary::new(http).with_base_url(server.url()).unwrap()
}
