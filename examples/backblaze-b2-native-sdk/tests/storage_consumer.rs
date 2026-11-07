//! Consumer of the public storage scope: bounded discovery, resume, delete, list again.

mod storage_support;

use std::{num::NonZeroUsize, sync::Mutex, time::Duration};

use concord_example_testserver::Recorded;
use concord_sdk_backblaze_b2_native::{FileName, FileNameQuery};
use serde_json::json;
use storage_support::*;

#[tokio::test]
async fn caller_resumes_a_listing_deletes_a_discovered_version_and_lists_again() {
    let present = Mutex::new(true);
    let fixture = Fixture::start(move |request: &Recorded| match request.path() {
        "/issued/api/b2api/v4/b2_list_file_names" => {
            assert_eq!(request.query_all("delimiter"), ["/"]);
            assert_eq!(request.query_all("maxFileCount"), ["2"]);
            if !*present.lock().unwrap() {
                assert_eq!(request.query_all("prefix"), [NAME]);
                page(vec![], None)
            } else if request.query_all("startFileName").is_empty() {
                assert_eq!(request.query_all("prefix"), ["reports/"]);
                page(vec![folder()], Some(NAME))
            } else {
                assert_eq!(request.query_all("prefix"), ["reports/"]);
                assert_eq!(request.query_all("startFileName"), [NAME]);
                page(vec![file()], None)
            }
        }
        "/issued/api/b2api/v4/b2_delete_file_version" => {
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&request.body).unwrap(),
                json!({"fileName": NAME, "fileId": ID})
            );
            *present.lock().unwrap() = false;
            deleted()
        }
        other => panic!("unexpected endpoint {other}"),
    })
    .await;
    let session = fixture.session(Duration::from_secs(3)).await;
    assert!(session.account().allowed().has_capability("listFiles"));
    assert!(session.account().allowed().has_capability("deleteFiles"));
    let storage = session.storage();
    let mut names = storage.list_file_names(query(), NonZeroUsize::new(1).unwrap());
    let folder = names.next_page().await.unwrap().unwrap();
    assert_eq!(folder.files[0].file_id, None);
    assert!(names.next_page().await.unwrap().is_none());
    assert!(!names.is_complete());
    let resume = names.resume_point().unwrap();
    let mut names = storage.list_file_names(resume, NonZeroUsize::new(2).unwrap());
    let page = names.next_page().await.unwrap().unwrap();
    assert!(names.is_complete());
    let version = &page.files[0];
    let name = FileName::new(version.file_name.clone()).unwrap();
    let id = version.file_id.as_ref().unwrap();
    let deleted = storage.delete_file_version(&name, id).await.unwrap();
    assert_eq!(&deleted.file_id, id);
    assert_eq!(deleted.file_name, name);
    let mut verify = storage.list_file_names(
        FileNameQuery {
            prefix: Some(name.as_str().into()),
            ..query()
        },
        NonZeroUsize::new(1).unwrap(),
    );
    assert!(verify.next_page().await.unwrap().unwrap().files.is_empty());
    assert!(verify.is_complete());
    assert_eq!(fixture.api.requests().len(), 4);
    fixture.shutdown().await;
}
