//! Independent parallel-results fixtures; no reference application workflows are used.

use std::{
    collections::BTreeMap,
    fs, io,
    num::NonZeroUsize,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use concord_check_dx03_fanout_sink::{
    stage1::AccountRow,
    stage2::{FilePart, FileSink, PartError, PartSink, RunLimits, StagedPart, export},
};
use concord_example_salesforce_sdk::{
    OrgContext, ProtocolErrorKind, SfError,
    ids::{AccessToken, ApiVersion, InstanceUrl, QueryJobId},
};
use concord_example_testserver::{Body, Recorded, Reply, TestServer};
use serde_json::json;

const JOB: &str = "750000000000001AAA";
const JOB_PATH: &str = "/services/data/v68.0/jobs/query/750000000000001AAA";
const MANIFEST_PATH: &str = "/services/data/v68.0/jobs/query/750000000000001AAA/resultPages";
const RESULT_PATH: &str = "/services/data/v68.0/jobs/query/750000000000001AAA/results";
const HEADER: &str = "Id|Name|NumberOfEmployees\r\n";
const ROWS: [&str; 6] = [
    "001000000000001AAA|\"Café | \"\"North\"\"\r\nBranch\"|42\r\n",
    "001000000000002AAA|\"  Padded  \"|\r\n",
    "001000000000003AAA|Third|3\r\n",
    "001000000000004AAA|Fourth|4\r\n",
    "001000000000005AAA|Équipe 😀|\"\"\r\n",
    "001000000000006AAA|Last|6\r\n",
];

fn limits(requests: usize, manifests: usize) -> RunLimits {
    RunLimits {
        concurrency: NonZeroUsize::new(2).unwrap(),
        result_requests: NonZeroUsize::new(requests).unwrap(),
        manifest_requests: NonZeroUsize::new(manifests).unwrap(),
    }
}

fn info() -> Reply {
    Reply::json(200, serde_json::to_vec(&json!({
        "id": JOB, "operation": "query", "object": "Account", "state": "JobComplete",
        "apiVersion": 68.0, "contentType": "CSV", "columnDelimiter": "PIPE", "lineEnding": "CRLF"
    })).unwrap())
}

fn manifest(parts: &[usize], more: bool) -> Reply {
    Reply::json(
        200,
        serde_json::to_vec(&json!({
            "resultPages": parts.iter().map(|part| json!({
                "resultUrl": format!("{RESULT_PATH}?locator=p{part}&maxRecords=2")
            })).collect::<Vec<_>>(),
            // The documented singular alias and lowercase returned resource spelling.
            "nextRecordUrl": more.then(|| format!("{JOB_PATH}/resultpages?locator=next_manifest")),
            "done": !more
        }))
        .unwrap(),
    )
}

fn csv(body: &str, declared: Option<u64>) -> Reply {
    let reply = Reply::text(200, "text/csv; charset=UTF-8; header=present", body);
    match declared {
        Some(count) => reply.header("Sforce-NumberOfRecords", count.to_string()),
        None => reply, // Parallel responses need neither count nor Sforce-Locator.
    }
}

fn full_part(part: usize) -> Reply {
    csv(
        &format!("{HEADER}{}{}", ROWS[part * 2], ROWS[part * 2 + 1]),
        Some(2),
    )
}

fn context(server: &TestServer) -> OrgContext {
    OrgContext::new(
        concord::reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
        InstanceUrl::parse(&server.url()).unwrap(),
        ApiVersion::PINNED,
        AccessToken::new("fixture-token").unwrap(),
    )
    .unwrap()
}

fn job_id() -> QueryJobId {
    QueryJobId::new(JOB).unwrap()
}

fn part_number(request: &Recorded) -> usize {
    assert_eq!(request.path(), RESULT_PATH);
    assert_eq!(request.query_all("maxRecords"), ["2"]);
    request.query_all("locator")[0]
        .strip_prefix('p')
        .unwrap()
        .parse()
        .unwrap()
}

fn downloaded(server: &TestServer) -> Vec<usize> {
    server
        .requests()
        .iter()
        .filter(|r| r.path() == RESULT_PATH)
        .map(part_number)
        .collect()
}

static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        // Keep even transient fixture files inside this package.
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/stage2-fixtures");
        fs::create_dir_all(&parent).unwrap();
        Self(parent.join(format!(
            "export-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::SeqCst)
        )))
    }
    fn sink(&self) -> FileSink {
        FileSink::create(&self.0).unwrap()
    }
    fn assert_no_staging(&self) {
        assert!(
            fs::read_dir(&self.0)
                .unwrap()
                .all(|entry| entry.unwrap().path().extension().unwrap() == "jsonl")
        );
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct Progress {
    rows: Mutex<BTreeMap<u64, usize>>,
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Progress {
    fn rows(&self, ordinal: u64) -> usize {
        *self.rows.lock().unwrap().get(&ordinal).unwrap_or(&0)
    }
}

struct ObservedSink {
    files: FileSink,
    progress: Arc<Progress>,
    fail_second_row_once: Arc<AtomicBool>,
}

struct ObservedPart {
    file: Option<FilePart>,
    ordinal: u64,
    rows: usize,
    progress: Arc<Progress>,
    fail_second_row_once: Arc<AtomicBool>,
}

impl PartSink for ObservedSink {
    type Part = ObservedPart;
    fn begin(&self, ordinal: u64) -> io::Result<ObservedPart> {
        let file = self.files.begin(ordinal)?;
        let active = self.progress.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.progress.peak.fetch_max(active, Ordering::SeqCst);
        Ok(ObservedPart {
            file: Some(file),
            ordinal,
            rows: 0,
            progress: self.progress.clone(),
            fail_second_row_once: self.fail_second_row_once.clone(),
        })
    }
}

impl StagedPart for ObservedPart {
    fn push(&mut self, row: &AccountRow) -> io::Result<()> {
        if self.rows == 1 && self.fail_second_row_once.swap(false, Ordering::SeqCst) {
            return Err(io::Error::other("injected sink failure"));
        }
        self.file.as_mut().unwrap().push(row)?;
        self.rows += 1;
        *self
            .progress
            .rows
            .lock()
            .unwrap()
            .entry(self.ordinal)
            .or_default() += 1;
        Ok(())
    }
    fn publish(mut self) -> io::Result<()> {
        self.file.take().unwrap().publish()
    }
}

impl Drop for ObservedPart {
    fn drop(&mut self) {
        self.progress.active.fetch_sub(1, Ordering::SeqCst);
    }
}

fn assert_full_output(bytes: &[u8]) {
    let rows: Vec<AccountRow> = bytes
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    let expected = [
        ("001000000000001AAA", "Café | \"North\"\r\nBranch", Some(42)),
        ("001000000000002AAA", "  Padded  ", None),
        ("001000000000003AAA", "Third", Some(3)),
        ("001000000000004AAA", "Fourth", Some(4)),
        ("001000000000005AAA", "Équipe 😀", None),
        ("001000000000006AAA", "Last", Some(6)),
    ];
    assert_eq!(rows.len(), expected.len());
    for (row, (id, name, employees)) in rows.iter().zip(expected) {
        assert_eq!(row.id.as_str(), id);
        assert_eq!(row.name, name);
        assert_eq!(row.number_of_employees, employees);
    }
}

#[tokio::test]
async fn stalled_part_holds_slot_failed_part_is_not_published_and_resume_only_redoes_it() {
    let directory = TestDirectory::new();
    let progress = Arc::new(Progress::default());
    let sink = ObservedSink {
        files: directory.sink(),
        progress: progress.clone(),
        fail_second_row_once: Arc::new(AtomicBool::new(false)),
    };
    let published: Vec<_> = (0..3).map(|n| sink.files.published_path(n)).collect();
    let release_stall = Arc::new(AtomicBool::new(false));
    let fail_connection = Arc::new(AtomicBool::new(false));
    let bad_attempts = Arc::new(AtomicUsize::new(0));
    let release = release_stall.clone();
    let fail = fail_connection.clone();
    let attempts = bad_attempts.clone();
    let server = TestServer::start(move |request| {
        assert_eq!(request.method, "GET");
        assert_eq!(
            request.header("authorization"),
            Some("Bearer fixture-token")
        );
        match request.path() {
            JOB_PATH => info(),
            MANIFEST_PATH => {
                if request.query_all("locator").is_empty() {
                    manifest(&[0, 1], true)
                } else {
                    manifest(&[2], false)
                }
            }
            RESULT_PATH => match part_number(request) {
                0 => {
                    let release = release.clone();
                    let mut step = 0;
                    csv("", Some(2)).with_body(Body::Generate(
                        Box::new(move || {
                            if step == 0 {
                                step = 1;
                                return Some(format!("{HEADER}{}", ROWS[0]).into());
                            }
                            if !release.load(Ordering::SeqCst) {
                                return Some("\n".into());
                            }
                            if step == 1 {
                                step = 2;
                                return Some(ROWS[1].into());
                            }
                            None
                        }),
                        Some(Duration::from_millis(1)),
                    ))
                }
                1 if attempts.fetch_add(1, Ordering::SeqCst) == 0 => {
                    let fail = fail.clone();
                    let mut first = true;
                    csv("", Some(2)).with_body(Body::GenerateThenFail(Box::new(move || {
                        if first {
                            first = false;
                            return Some(format!("{HEADER}{}", ROWS[2]).into());
                        }
                        if fail.load(Ordering::SeqCst) {
                            None
                        } else {
                            Some("\n".into())
                        }
                    })))
                }
                1 => full_part(1),
                2 => csv(&format!("{HEADER}{}{}", ROWS[4], ROWS[5]), None),
                n => panic!("unexpected part {n}"),
            },
            path => panic!("unexpected path {path}"),
        }
    })
    .await;
    let ctx = context(&server);
    let control = async {
        assert!(
            server
                .wait_until(Duration::from_secs(2), |s| {
                    progress.rows(0) == 1
                        && progress.rows(1) == 1
                        && s.requests()
                            .iter()
                            .filter(|r| r.path() == MANIFEST_PATH)
                            .count()
                            == 2
                })
                .await
        );
        assert_eq!(progress.active.load(Ordering::SeqCst), 2);
        assert_eq!(progress.peak.load(Ordering::SeqCst), 2);
        let mut requested = downloaded(&server);
        requested.sort();
        assert_eq!(
            requested,
            [0, 1],
            "third part started while both slots were occupied"
        );
        assert!(
            published.iter().all(|path| !path.exists()),
            "a prefix was published before EOF"
        );
        fail_connection.store(true, Ordering::SeqCst);
        assert!(
            server
                .wait_until(Duration::from_secs(2), |_| published[2].exists())
                .await
        );
        assert!(!published[0].exists(), "stalled prefix must remain staged");
        assert!(
            !published[1].exists(),
            "failed prefix must never be published"
        );
        assert_eq!(progress.active.load(Ordering::SeqCst), 1);
        assert_eq!(progress.peak.load(Ordering::SeqCst), 2);
        release_stall.store(true, Ordering::SeqCst);
    };
    let (report, ()) = tokio::join!(export(&ctx, job_id(), sink, limits(10, 10)), control);
    let report = report.unwrap();
    assert!(!report.is_complete());
    assert_eq!(
        report.completed().map(|p| p.ordinal).collect::<Vec<_>>(),
        [0, 2]
    );
    assert_eq!(
        report.redo().iter().map(|p| p.ordinal).collect::<Vec<_>>(),
        [1]
    );
    assert!(report.pending_discovery().is_none());
    assert_eq!(report.result_requests, 3);
    assert_eq!(report.discovery_requests, 2);
    assert_eq!(report.failures.len(), 1);
    assert_eq!(report.failures[0].part.ordinal, 1);
    let PartError::Salesforce(SfError::Read(error)) = &report.failures[0].error else {
        panic!("expected typed mid-body read error");
    };
    assert_eq!(error.kind(), concord::stream::ReadErrorKind::Transport);
    assert_eq!(error.position().records, 1);
    assert_eq!(
        progress.rows(1),
        1,
        "one row must have reached the sink before failure"
    );
    assert_eq!(
        report
            .completed()
            .find(|r| r.ordinal == 2)
            .unwrap()
            .declared_records,
        None
    );
    directory.assert_no_staging();
    let before = server.requests().len();

    let report = report.resume(limits(10, 10)).await;

    assert!(report.is_complete());
    assert!(report.failures.is_empty());
    assert_eq!(report.result_requests, 1);
    assert_eq!(report.discovery_requests, 0);
    let resumed = &server.requests()[before..];
    assert_eq!(resumed.len(), 1);
    assert_eq!(part_number(&resumed[0]), 1);
    assert_eq!(
        report.completed().map(|p| p.ordinal).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(report.completed().all(|p| p.rows == 2));
    let mut bytes = Vec::new(); // Only the test collects output for equality assertions.
    report
        .sink()
        .files
        .copy_completed(report.completed(), &mut bytes)
        .unwrap();
    assert_full_output(&bytes);
    directory.assert_no_staging();
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn bounded_resume_preserves_queued_parts_and_undiscovered_manifests() {
    let directory = TestDirectory::new();
    let server = TestServer::start(|request| match request.path() {
        JOB_PATH => info(),
        MANIFEST_PATH => {
            if request.query_all("locator").is_empty() {
                manifest(&[0, 1], true)
            } else {
                manifest(&[2], false)
            }
        }
        RESULT_PATH => full_part(part_number(request)),
        path => panic!("unexpected path {path}"),
    })
    .await;
    let report = export(&context(&server), job_id(), directory.sink(), limits(1, 1))
        .await
        .unwrap();
    assert_eq!(
        report.completed().map(|p| p.ordinal).collect::<Vec<_>>(),
        [0]
    );
    assert_eq!(
        report.redo().iter().map(|p| p.ordinal).collect::<Vec<_>>(),
        [1]
    );
    assert!(report.pending_discovery().is_some());

    let report = report.resume(limits(1, 1)).await;
    assert!(!report.is_complete());
    assert!(report.redo().is_empty());
    assert!(report.pending_discovery().is_some());
    assert_eq!(report.result_requests, 1);
    assert_eq!(
        report.discovery_requests, 0,
        "retry phase must consume the shared result budget"
    );

    let report = report.resume(limits(1, 1)).await;
    assert!(report.is_complete());
    assert_eq!(report.result_requests, 1);
    assert_eq!(report.discovery_requests, 1);
    assert_eq!(downloaded(&server), [0, 1, 2]);
    let manifests: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|r| r.path() == MANIFEST_PATH)
        .collect();
    assert_eq!(manifests.len(), 2);
    assert_eq!(manifests[1].query_all("locator"), ["next_manifest"]);
    let mut bytes = Vec::new();
    report
        .sink()
        .copy_completed(report.completed(), &mut bytes)
        .unwrap();
    assert_full_output(&bytes);
    let before = server.requests().len();
    let report = report.resume(limits(1, 1)).await;
    assert!(report.is_complete());
    assert_eq!(report.result_requests, 0);
    assert_eq!(server.requests().len(), before);
    directory.assert_no_staging();
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn discovery_failure_resumes_at_failed_manifest_without_redownloading_completed_parts() {
    let directory = TestDirectory::new();
    let next_calls = AtomicUsize::new(0);
    let server = TestServer::start(move |request| match request.path() {
        JOB_PATH => info(),
        MANIFEST_PATH if request.query_all("locator").is_empty() => manifest(&[0], true),
        MANIFEST_PATH => {
            if next_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                Reply::json(
                    500,
                    br#"[{"errorCode":"UNKNOWN_EXCEPTION","message":"fixture discovery failure"}]"#,
                )
            } else {
                manifest(&[1, 2], false)
            }
        }
        RESULT_PATH => full_part(part_number(request)),
        path => panic!("unexpected path {path}"),
    })
    .await;
    let report = export(
        &context(&server),
        job_id(),
        directory.sink(),
        limits(10, 10),
    )
    .await
    .unwrap();
    assert!(!report.is_complete());
    assert_eq!(
        report.completed().map(|p| p.ordinal).collect::<Vec<_>>(),
        [0]
    );
    assert!(report.redo().is_empty());
    assert!(report.pending_discovery().is_some());
    assert!(
        report
            .discovery_error
            .as_ref()
            .unwrap()
            .has_error_code("UNKNOWN_EXCEPTION")
    );
    let before = server.requests().len();

    let report = report.resume(limits(10, 10)).await;

    assert!(report.is_complete());
    assert!(report.discovery_error.is_none());
    assert_eq!(report.discovery_requests, 1);
    assert_eq!(report.result_requests, 2);
    let resumed = &server.requests()[before..];
    assert_eq!(resumed[0].path(), MANIFEST_PATH);
    assert_eq!(resumed[0].query_all("locator"), ["next_manifest"]);
    let mut requested = downloaded(&server);
    requested.sort();
    assert_eq!(requested, [0, 1, 2]);
    let mut bytes = Vec::new();
    report
        .sink()
        .copy_completed(report.completed(), &mut bytes)
        .unwrap();
    assert_full_output(&bytes);
    directory.assert_no_staging();
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn a_count_failure_at_eof_discards_all_staged_rows_before_resume() {
    let directory = TestDirectory::new();
    let attempts = AtomicUsize::new(0);
    let server = TestServer::start(move |request| match request.path() {
        JOB_PATH => info(),
        MANIFEST_PATH => manifest(&[0], false),
        RESULT_PATH => csv(
            &format!("{HEADER}{}{}", ROWS[0], ROWS[1]),
            Some(if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                3
            } else {
                2
            }),
        ),
        path => panic!("unexpected path {path}"),
    })
    .await;
    let report = export(
        &context(&server),
        job_id(),
        directory.sink(),
        limits(10, 10),
    )
    .await
    .unwrap();
    assert_eq!(report.completed().count(), 0);
    assert!(!report.sink().published_path(0).exists());
    let PartError::Salesforce(SfError::Protocol(error)) = &report.failures[0].error else {
        panic!("expected count validation failure");
    };
    assert!(matches!(
        error.kind(),
        ProtocolErrorKind::RecordCountMismatch {
            declared: 3,
            parsed: 2
        }
    ));
    directory.assert_no_staging();
    let report = report.resume(limits(10, 10)).await;
    assert!(report.is_complete());
    assert_eq!(report.completed().next().unwrap().rows, 2);
    assert_eq!(downloaded(&server), [0, 0]);
    directory.assert_no_staging();
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn sink_failure_does_not_publish_a_prefix_and_can_be_retried() {
    let directory = TestDirectory::new();
    let progress = Arc::new(Progress::default());
    let sink = ObservedSink {
        files: directory.sink(),
        progress: progress.clone(),
        fail_second_row_once: Arc::new(AtomicBool::new(true)),
    };
    let server = TestServer::start(|request| match request.path() {
        JOB_PATH => info(),
        MANIFEST_PATH => manifest(&[0], false),
        RESULT_PATH => full_part(0),
        path => panic!("unexpected path {path}"),
    })
    .await;
    let report = export(&context(&server), job_id(), sink, limits(10, 10))
        .await
        .unwrap();
    assert_eq!(progress.rows(0), 1);
    assert_eq!(report.completed().count(), 0);
    assert!(!report.sink().files.published_path(0).exists());
    assert!(matches!(report.failures[0].error, PartError::Sink(_)));
    directory.assert_no_staging();
    let report = report.resume(limits(10, 10)).await;
    assert!(report.is_complete());
    assert_eq!(report.completed().next().unwrap().rows, 2);
    assert_eq!(report.result_requests, 1);
    assert_eq!(report.discovery_requests, 0);
    directory.assert_no_staging();
    assert_eq!(server.shutdown().await.open_after, 0);
}
