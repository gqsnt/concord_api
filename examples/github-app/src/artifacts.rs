//! GH-B — workflow artifact retrieval.
//!
//! **Input:** a [`RepoContext`], a [`RunFilter`] and [`RunSelector`], an artifact name, page
//! budgets, [`DownloadLimits`] and a bounded [`ArtifactSink`]. **Output:** a [`Retrieval`]: the
//! request log and either a [`RetrievalReport`] (the selected run, its jobs, the artifact and a
//! [`DownloadReceipt`]) or a typed incomplete outcome.
//!
//! Steps (serial):
//!
//! 1. Walk the workflow runs (`GET .../actions/runs` with the filter; items moved out of the
//!    `workflow_runs` envelope) page by page (the SDK's `pages()`, capped by the run budget)
//!    until a run matches the selector; stop there (an early stop, not exhaustion). The run, job
//!    and artifact walks read their end from the traversal: `Ok(None)` is exhaustion, the
//!    budget reached while GitHub lists more is reported as such (`exhausted: false`, a resume
//!    link, `complete: false`), a failed page is [`RetrievalOutcome::Incomplete`].
//! 2. Fetch that run (`GET .../actions/runs/{id}`) and walk all of its jobs (`jobs` envelope).
//! 3. Walk the run's artifacts filtered by name (`artifacts` envelope) and take the first with
//!    exactly that name, then fetch it (`GET .../actions/artifacts/{id}`) for a current `expired`
//!    flag and size.
//! 4. An expired artifact (`expired: true`, or `410` from the download endpoint) is
//!    [`RetrievalOutcome::Expired`]: nothing is downloaded.
//! 5. Locate the download (`302` at the native boundary, see the SDK's `download` module) and
//!    stream it into the sink with the byte limit. The token is never sent to the download host.
//!    On success the sink commits; on any transfer failure or cancellation it aborts (partial
//!    output removed) and the outcome says how many bytes had been written.

use std::fmt;
use std::num::NonZeroUsize;

use concord_example_github_sdk::download::{
    DownloadLimits, LocateError, Located, StreamError, StreamReceipt, locate, stream_to,
};
use concord_example_github_sdk::filters::RunFilter;
use concord_example_github_sdk::ids::{ArtifactId, JobId, RunId};
use concord_example_github_sdk::meta::send_with_meta;
use concord_example_github_sdk::models::{Artifact, Conclusion, Job, RunState, WorkflowRun};
use concord_example_github_sdk::paging::{NextLink, operation};
use concord_example_github_sdk::params::Timestamp;
use concord_example_github_sdk::{GitHubError, RepoContext};
use concord_pagination::{PagesError, Resume};

use crate::requests::{RequestLog, RequestRecord, record_next};
use crate::sink::ArtifactSink;

/// Which run to select: the first one GitHub lists (newest first) that is completed and, when
/// given, has this conclusion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunSelector {
    /// Required conclusion (`success`, say); `None` accepts any completed run.
    pub conclusion: Option<Conclusion>,
}

impl RunSelector {
    fn matches(&self, run: &WorkflowRun) -> bool {
        run.status == Some(RunState::Completed)
            && self
                .conclusion
                .as_ref()
                .is_none_or(|wanted| run.conclusion.as_ref() == Some(wanted))
    }
}

/// Page budgets of one retrieval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetrievalBudget {
    /// Workflow-run pages to scan for a matching run.
    pub run_pages: NonZeroUsize,
    /// Job pages of the selected run.
    pub job_pages: NonZeroUsize,
    /// Artifact pages of the selected run.
    pub artifact_pages: NonZeroUsize,
}

/// What to retrieve.
#[derive(Clone, Debug)]
pub struct RetrievalInput {
    /// Run filters (branch, event, status, ...).
    pub runs: RunFilter,
    /// Which listed run to take.
    pub select: RunSelector,
    /// The artifact's exact name.
    pub artifact_name: String,
    /// Page budgets.
    pub budget: RetrievalBudget,
    /// Download byte limit.
    pub limits: DownloadLimits,
}

/// The selected run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunReport {
    /// Run id.
    pub id: RunId,
    /// Run number.
    pub run_number: u64,
    /// Head branch.
    pub head_branch: Option<String>,
    /// Event.
    pub event: String,
    /// Conclusion.
    pub conclusion: Option<Conclusion>,
    /// Runs inspected before it was found (itself included).
    pub runs_scanned: u64,
}

/// One job of the selected run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobReport {
    /// Job id.
    pub id: JobId,
    /// Name.
    pub name: String,
    /// Status.
    pub status: RunState,
    /// Conclusion.
    pub conclusion: Option<Conclusion>,
    /// Number of steps.
    pub steps: usize,
}

/// The jobs of the selected run, as far as the job budget reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobsReport {
    /// Jobs in GitHub's order.
    pub jobs: Vec<JobReport>,
    /// The envelope's `total_count` (last page seen).
    pub total_count: Option<u64>,
    /// `false` when the job budget stopped the walk while GitHub had more pages.
    pub complete: bool,
}

/// The selected artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactReport {
    /// Artifact id.
    pub id: ArtifactId,
    /// Name.
    pub name: String,
    /// Archive size GitHub reports.
    pub size_in_bytes: u64,
    /// Expired flag (from the single-artifact request).
    pub expired: bool,
    /// Expiry time.
    pub expires_at: Option<Timestamp>,
    /// Digest GitHub reports (not verified by this workflow).
    pub digest: Option<String>,
}

/// What the download produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadReceipt {
    /// The download host GitHub redirected to.
    pub host: String,
    /// `x-github-request-id` of the `302`.
    pub redirect_request_id: Option<String>,
    /// Bytes streamed into the sink and committed.
    pub bytes: u64,
    /// Chunks handed to the sink.
    pub chunks: u64,
    /// Content type of the download response.
    pub content_type: Option<String>,
    /// Whether `bytes` equals the artifact's `size_in_bytes`.
    pub size_matches: bool,
}

impl DownloadReceipt {
    fn new(stream: StreamReceipt, redirect_request_id: Option<String>, size: u64) -> Self {
        Self {
            host: stream.host,
            redirect_request_id,
            bytes: stream.bytes,
            chunks: stream.chunks,
            content_type: stream.content_type,
            size_matches: stream.bytes == size,
        }
    }
}

/// A complete retrieval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetrievalReport {
    /// The selected run.
    pub run: RunReport,
    /// Its jobs.
    pub jobs: JobsReport,
    /// The artifact.
    pub artifact: ArtifactReport,
    /// The download.
    pub receipt: DownloadReceipt,
}

/// Why an artifact counts as expired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpirySignal {
    /// The artifact's `expired` flag; no download was attempted.
    Metadata,
    /// The download endpoint answered `410 Gone`.
    Gone,
}

/// The stage at which a retrieval stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Listing runs.
    Runs,
    /// Fetching the selected run.
    Run,
    /// Listing its jobs.
    Jobs,
    /// Listing its artifacts.
    Artifacts,
    /// Fetching the artifact.
    Artifact,
    /// Asking GitHub for the download location.
    Locate,
    /// Streaming the archive.
    Download,
    /// Committing the sink.
    Commit,
}

/// Why a retrieval stopped. The underlying errors are kept typed.
#[non_exhaustive]
pub enum RetrievalError<E> {
    /// A GitHub API request failed.
    GitHub(GitHubError),
    /// The download location could not be obtained.
    Locate(LocateError),
    /// The transfer failed (transport, byte limit, the sink's refusal or cancellation).
    Stream(StreamError<E>),
    /// The sink could not commit.
    Commit(E),
}

impl<E> fmt::Display for RetrievalError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub(error) => write!(f, "artifact retrieval: {error}"),
            Self::Locate(error) => write!(f, "artifact retrieval: {error}"),
            Self::Stream(error) => write!(f, "artifact retrieval: {error}"),
            Self::Commit(_) => f.write_str("artifact retrieval: the sink could not commit"),
        }
    }
}

impl<E> std::error::Error for RetrievalError<E> {}

/// What a retrieval had established before it stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Partial {
    /// The selected run.
    pub run: Option<RunReport>,
    /// Its jobs.
    pub jobs: Option<JobsReport>,
    /// The artifact.
    pub artifact: Option<ArtifactReport>,
    /// Bytes the sink had accepted before it was aborted (its partial output is removed).
    pub written: u64,
}

/// How a retrieval ended.
pub enum RetrievalOutcome<E> {
    /// Downloaded and committed.
    Retrieved(RetrievalReport),
    /// No listed run matched within the run budget.
    NoMatchingRun {
        /// Runs inspected.
        runs_scanned: u64,
        /// `true` when the listing was exhausted; `false` when the run budget stopped the scan.
        exhausted: bool,
        /// Where to continue scanning when bounded.
        resume: Option<Resume<NextLink>>,
    },
    /// The run has no artifact of that name (within the artifact budget).
    NoMatchingArtifact {
        /// The run.
        run: RunReport,
        /// Its jobs.
        jobs: JobsReport,
        /// `true` when the artifact listing was exhausted.
        exhausted: bool,
    },
    /// The artifact exists but its bytes are gone.
    Expired {
        /// The run.
        run: RunReport,
        /// Its jobs.
        jobs: JobsReport,
        /// The artifact.
        artifact: ArtifactReport,
        /// How the expiry was observed.
        signal: ExpirySignal,
    },
    /// A failure stopped the retrieval.
    Incomplete {
        /// Where.
        stage: Stage,
        /// Why.
        error: RetrievalError<E>,
        /// What was established.
        partial: Box<Partial>,
    },
}

impl<E> fmt::Debug for RetrievalOutcome<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retrieved(report) => f.debug_tuple("Retrieved").field(report).finish(),
            Self::NoMatchingRun {
                runs_scanned,
                exhausted,
                resume,
            } => f
                .debug_struct("NoMatchingRun")
                .field("runs_scanned", runs_scanned)
                .field("exhausted", exhausted)
                .field("resume", resume)
                .finish(),
            Self::NoMatchingArtifact {
                run,
                jobs,
                exhausted,
            } => f
                .debug_struct("NoMatchingArtifact")
                .field("run", run)
                .field("jobs", jobs)
                .field("exhausted", exhausted)
                .finish(),
            Self::Expired {
                run,
                jobs,
                artifact,
                signal,
            } => f
                .debug_struct("Expired")
                .field("run", run)
                .field("jobs", jobs)
                .field("artifact", artifact)
                .field("signal", signal)
                .finish(),
            Self::Incomplete {
                stage,
                error,
                partial,
            } => f
                .debug_struct("Incomplete")
                .field("stage", stage)
                .field("error", error)
                .field("partial", partial)
                .finish(),
        }
    }
}

/// A retrieval's result: the outcome and every request sent.
#[derive(Debug)]
pub struct Retrieval<E> {
    /// How it ended.
    pub outcome: RetrievalOutcome<E>,
    /// The API requests sent (the download host request is not an API request and carries no
    /// request id; its host is in the receipt).
    pub requests: Vec<RequestRecord>,
}

/// Runs the retrieval, streaming the archive into `sink`.
pub async fn retrieve_artifact<S: ArtifactSink>(
    repo: &RepoContext,
    input: &RetrievalInput,
    sink: &mut S,
) -> Retrieval<S::Error> {
    let log = RequestLog::default();
    let outcome = run(repo, input, sink, &log).await;
    Retrieval {
        outcome,
        requests: log.take(),
    }
}

fn incomplete<E>(stage: Stage, error: RetrievalError<E>, partial: Partial) -> RetrievalOutcome<E> {
    RetrievalOutcome::Incomplete {
        stage,
        error,
        partial: Box::new(partial),
    }
}

async fn run<S: ArtifactSink>(
    repo: &RepoContext,
    input: &RetrievalInput,
    sink: &mut S,
    log: &RequestLog,
) -> RetrievalOutcome<S::Error> {
    let mut partial = Partial::default();

    // 1. Scan runs until one matches.
    let mut runs = repo
        .run_pages(repo.runs(&input.runs))
        .pages()
        .max_pages(input.budget.run_pages);
    let mut scanned = 0u64;
    let listed = 'scan: loop {
        let next = runs.next_page().await;
        record_next(&next, operation::RUNS, log);
        let page = match next {
            Ok(Some(page)) => page,
            Ok(None) => {
                return RetrievalOutcome::NoMatchingRun {
                    runs_scanned: scanned,
                    exhausted: true,
                    resume: None,
                };
            }
            Err(PagesError::Provider { error, .. }) => {
                return incomplete(Stage::Runs, RetrievalError::GitHub(error), partial);
            }
            // The run budget, reached while GitHub lists more runs: continue from there later.
            Err(bounded) => {
                return RetrievalOutcome::NoMatchingRun {
                    runs_scanned: scanned,
                    exhausted: false,
                    resume: bounded.resume().cloned(),
                };
            }
        };
        for run in page.items {
            scanned += 1;
            if input.select.matches(&run) {
                break 'scan run;
            }
        }
    };

    // 2. The run itself, then all of its jobs.
    let run = match fetch(
        log,
        "actions/get-workflow-run",
        repo.actions().run(listed.id).prepare(),
    )
    .await
    {
        Ok(run) => run,
        Err(error) => return incomplete(Stage::Run, RetrievalError::GitHub(error), partial),
    };
    let run_report = RunReport {
        id: run.id,
        run_number: run.run_number,
        head_branch: run.head_branch,
        event: run.event,
        conclusion: run.conclusion,
        runs_scanned: scanned,
    };
    partial.run = Some(run_report.clone());
    let mut job_pages = repo
        .run_job_pages(run.id, |op| op)
        .pages()
        .max_pages(input.budget.job_pages);
    let mut jobs = JobsReport {
        jobs: Vec::new(),
        total_count: None,
        complete: false,
    };
    loop {
        let next = job_pages.next_page().await;
        record_next(&next, operation::RUN_JOBS, log);
        let page = match next {
            Ok(Some(page)) => page,
            Ok(None) => {
                jobs.complete = true;
                break;
            }
            Err(PagesError::Provider { error, .. }) => {
                return incomplete(Stage::Jobs, RetrievalError::GitHub(error), partial);
            }
            // The job budget, reached while GitHub lists more jobs: `complete` stays `false`.
            Err(_bounded) => break,
        };
        jobs.total_count = page.meta.total_count;
        jobs.jobs
            .extend(page.items.into_iter().map(|job: Job| JobReport {
                id: job.id,
                name: job.name,
                status: job.status,
                conclusion: job.conclusion,
                steps: job.steps.len(),
            }));
    }
    partial.jobs = Some(jobs.clone());

    // 3. The artifact by name, then its current state.
    let name = input.artifact_name.clone();
    let mut artifact_pages = repo
        .run_artifact_pages(run.id, |op| op.set_name(name.as_str()))
        .pages()
        .max_pages(input.budget.artifact_pages);
    let (found, exhausted) = loop {
        let next = artifact_pages.next_page().await;
        record_next(&next, operation::RUN_ARTIFACTS, log);
        let page = match next {
            Ok(Some(page)) => page,
            Ok(None) => break (None, true),
            Err(PagesError::Provider { error, .. }) => {
                return incomplete(Stage::Artifacts, RetrievalError::GitHub(error), partial);
            }
            // The artifact budget, reached while GitHub lists more artifacts.
            Err(_bounded) => break (None, false),
        };
        let found = page
            .items
            .into_iter()
            .find(|a| a.name == input.artifact_name);
        if found.is_some() {
            break (found, false);
        }
    };
    let Some(listed) = found else {
        return RetrievalOutcome::NoMatchingArtifact {
            run: run_report,
            jobs,
            exhausted,
        };
    };
    let artifact: Artifact = match fetch(
        log,
        "actions/get-artifact",
        repo.actions().artifact(listed.id).prepare(),
    )
    .await
    {
        Ok(artifact) => artifact,
        Err(error) => return incomplete(Stage::Artifact, RetrievalError::GitHub(error), partial),
    };
    let artifact_report = ArtifactReport {
        id: artifact.id,
        name: artifact.name,
        size_in_bytes: artifact.size_in_bytes,
        expired: artifact.expired,
        expires_at: artifact.expires_at,
        digest: artifact.digest,
    };
    partial.artifact = Some(artifact_report.clone());
    if artifact_report.expired {
        return RetrievalOutcome::Expired {
            run: run_report,
            jobs,
            artifact: artifact_report,
            signal: ExpirySignal::Metadata,
        };
    }
    if artifact_report.size_in_bytes > input.limits.max_bytes {
        return incomplete(
            Stage::Download,
            RetrievalError::Stream(StreamError::TooLarge {
                declared: Some(artifact_report.size_in_bytes),
                written: 0,
                limit: input.limits.max_bytes,
            }),
            partial,
        );
    }

    // 4. Locate and stream.
    let location = match locate(repo, artifact_report.id).await {
        Ok(Located::Redirect(location)) => {
            log.meta("actions/download-artifact", location.meta());
            location
        }
        Ok(Located::Gone { meta }) => {
            log.meta("actions/download-artifact", &meta);
            return RetrievalOutcome::Expired {
                run: run_report,
                jobs,
                artifact: artifact_report,
                signal: ExpirySignal::Gone,
            };
        }
        Err(error) => {
            log.push(RequestRecord::of("actions/download-artifact", error.meta()));
            return incomplete(Stage::Locate, RetrievalError::Locate(error), partial);
        }
    };
    let redirect_request_id = location.meta().request_id().map(str::to_owned);
    let stream = match stream_to(repo.context().http(), &location, sink, input.limits).await {
        Ok(stream) => stream,
        Err(error) => {
            partial.written = error.written();
            sink.abort();
            return incomplete(Stage::Download, RetrievalError::Stream(error), partial);
        }
    };
    if let Err(error) = sink.commit() {
        partial.written = stream.bytes;
        sink.abort();
        return incomplete(Stage::Commit, RetrievalError::Commit(error), partial);
    }
    RetrievalOutcome::Retrieved(RetrievalReport {
        run: run_report,
        jobs,
        receipt: DownloadReceipt::new(stream, redirect_request_id, artifact_report.size_in_bytes),
        artifact: artifact_report,
    })
}

async fn fetch<T>(
    log: &RequestLog,
    operation: &'static str,
    prepared: Result<concord::Prepared<T>, concord::Error>,
) -> Result<T, GitHubError> {
    let result = match prepared {
        Ok(prepared) => send_with_meta(prepared).await,
        Err(error) => Err(GitHubError::from(error)),
    };
    match result {
        Ok(with) => {
            log.meta(operation, &with.meta);
            Ok(with.value)
        }
        Err(error) => {
            log.error(operation, &error);
            Err(error)
        }
    }
}

impl<E> fmt::Debug for RetrievalError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GitHub(error) => f.debug_tuple("GitHub").field(error).finish(),
            Self::Locate(error) => f.debug_tuple("Locate").field(error).finish(),
            Self::Stream(error) => f.debug_tuple("Stream").field(error).finish(),
            Self::Commit(_) => f.write_str("Commit(<sink error>)"),
        }
    }
}
