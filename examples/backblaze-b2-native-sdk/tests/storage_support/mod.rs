//! Synthetic v4 name-list/delete fixtures, independent of the SDK's encoders and models.

#![allow(dead_code)]

use std::time::Duration;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_backblaze_b2_native::{
    ApplicationKey, BucketId, Client, FileNameQuery, Session, http_client_builder,
};
use serde_json::{Value, json};

pub const BUCKET: &str = "fixture-bucket";
pub const TOKEN: &str = "fixture-account-token";
pub const NAME: &str = "reports/résumé +&?#%.txt";
pub const ID: &str = "fixture-version-1";

pub fn query() -> FileNameQuery {
    FileNameQuery {
        prefix: Some("reports/".into()),
        delimiter: Some("/".into()),
        max_file_count: Some(2),
        ..FileNameQuery::new(BucketId::new(BUCKET).unwrap())
    }
}

pub fn file() -> Value {
    json!({
        "accountId": "fixture-account", "bucketId": BUCKET, "action": "upload",
        "fileName": NAME, "fileId": ID, "contentLength": 6,
        "contentSha1": "f572d396fae9206628714fb2ce00f72e94f2258f",
        "contentType": "text/plain", "fileInfo": {}, "uploadTimestamp": 123
    })
}

pub fn folder() -> Value {
    json!({
        "accountId": "fixture-account", "bucketId": BUCKET, "action": "folder",
        "fileName": "reports/sub/", "fileId": null, "contentLength": 0,
        "contentSha1": null, "contentType": null, "fileInfo": {}, "uploadTimestamp": 0
    })
}

pub fn page(files: Vec<Value>, next: Option<&str>) -> Reply {
    Reply::json(
        200,
        json!({"files": files, "nextFileName": next}).to_string(),
    )
    .header("x-fixture-request-id", "storage-request")
}

pub fn deleted() -> Reply {
    Reply::json(200, json!({"fileName": NAME, "fileId": ID}).to_string())
        .header("x-fixture-request-id", "storage-request")
}

pub fn provider(status: u16, code: &str) -> Reply {
    Reply::json(
        status,
        json!({
            "status": status, "code": code, "message": "private provider message"
        })
        .to_string(),
    )
    .header("x-fixture-request-id", "storage-request")
}

pub struct Fixture {
    pub account: TestServer,
    pub api: TestServer,
    pub download: TestServer,
}

impl Fixture {
    pub async fn start(respond: impl Fn(&Recorded) -> Reply + Send + Sync + 'static) -> Self {
        let api = TestServer::start(move |request: &Recorded| {
            assert_eq!(request.header_all("authorization"), [TOKEN]);
            respond(request)
        })
        .await;
        let download =
            TestServer::start(|_: &Recorded| panic!("unexpected download request")).await;
        let api_url = format!("{}/issued/api", api.url());
        let download_url = download.url();
        let account =
            TestServer::start(move |request: &Recorded| {
                assert_eq!(request.path(), "/b2api/v4/b2_authorize_account");
                Reply::json(200, json!({
                "accountId": "fixture-account", "authorizationToken": TOKEN,
                "apiInfo": {"storageApi": {
                    "apiUrl": api_url, "downloadUrl": download_url,
                    "allowed": {"buckets": [{"id": BUCKET, "name": "fixture"}],
                        "capabilities": ["listFiles", "deleteFiles"], "namePrefix": "reports/"}
                }}
            }).to_string())
            })
            .await;
        Self {
            account,
            api,
            download,
        }
    }

    pub async fn session(&self, timeout: Duration) -> Session {
        let http = http_client_builder()
            .no_proxy()
            .timeout(timeout)
            .build()
            .unwrap();
        Client::new(http)
            .with_base_url(&self.account.url())
            .unwrap()
            .authorize_account(&ApplicationKey::new("fixture-id", "fixture-key").unwrap())
            .await
            .unwrap()
    }

    pub async fn shutdown(self) {
        assert_eq!(
            self.account.requests().len(),
            1,
            "no automatic reauthorization"
        );
        assert!(
            self.download.requests().is_empty(),
            "use the issued API origin"
        );
        for server in [self.account, self.api, self.download] {
            assert_eq!(server.shutdown().await.open_after, 0);
        }
    }
}
