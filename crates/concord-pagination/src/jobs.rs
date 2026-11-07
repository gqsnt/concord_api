//! Jobs discovered through a sequential manifest chain and run with a bounded window.

use std::collections::VecDeque;
use std::fmt;
use std::future::{Future, poll_fn};
use std::hash::Hash;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::repeated::KeyHistory;
use crate::types::{Budget, Checkpoint, Keyed, Manifest, OnError, Options, Outcome, Progress};
use crate::types::{Resume, Unfinished};
use crate::window::{Halt, Step, Window};

/// What a job traversal's `poll` returns.
type Polled<D, R, EJ, ED> = Poll<Option<Result<Keyed<D, R>, JobsError<D, EJ, ED>>>>;

/// Settings of [`manifest_jobs`]: the discovery budget, the descriptor backlog and the job
/// [`Options`] (job budget, window, order, error policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobOptions {
    discovery: Budget,
    backlog: Option<NonZeroUsize>,
    jobs: Options,
}

impl JobOptions {
    /// At most `discovery` manifest calls (empty manifests included) and the job settings
    /// `jobs`. The backlog bound defaults to the job window.
    pub const fn new(discovery: Budget, jobs: Options) -> Self {
        Self {
            discovery,
            backlog: None,
            jobs,
        }
    }

    /// The descriptor backlog bound: a manifest call is admitted only while fewer descriptors
    /// than this wait for a job slot. One manifest's descriptors are accepted whole, so at most
    /// `backlog - 1 +` (the largest manifest) descriptors are queued.
    pub const fn backlog(mut self, backlog: NonZeroUsize) -> Self {
        self.backlog = Some(backlog);
        self
    }
}

/// Discovers job descriptors through a sequential manifest chain starting at `from`, and runs a
/// job per descriptor with a bounded window.
///
/// `discover` receives the key of the manifest to fetch (`None` for the first) and returns a
/// [`Manifest`]: descriptors plus the next manifest's key. `job` receives one descriptor and
/// returns the *whole* job as one future: request acquisition, incremental parsing of the body,
/// and sink work through EOF, ending in a small completion result (a shard receipt, a record
/// count). The job keeps its window slot until that future completes or is dropped, so a response
/// whose headers arrived but whose body stalls still occupies its slot.
///
/// # Bounds
///
/// - Discovery calls: at most the discovery budget, counting empty manifests, so a repeating or
///   unproductive chain ends ([`Outcome::DiscoveryBounded`]).
/// - Discovery concurrency: one manifest call at a time (the chain is dependent), beside the job
///   window: at most `window + 1` futures are owned at once.
/// - Descriptor backlog: see [`JobOptions::backlog`]; no manifest is fetched while the backlog is
///   full, or while queued descriptors already cover the remaining job budget.
/// - Jobs: at most the job budget; jobs in flight plus completed results retained for ordered
///   emission never exceed the window.
///
/// Byte and record limits belong to the job's reader (the window is not a byte bound).
///
/// # Failures
///
/// A job failure follows [`Options::on_error`] as in [`independent`](crate::independent). A
/// discovery failure ends discovery (no next key is guessed) and is returned by `next` as
/// [`JobsError::Discovery`]. Then, with [`OnError::Stop`], every job in flight is dropped
/// (cancelled); with [`OnError::Continue`], the jobs already admitted *and* the descriptors
/// already queued still run to completion. Once that known work has finished, the outcome is
/// [`Outcome::DiscoveryFailed`]. If you call [`Jobs::finish`] earlier (while continue-mode jobs
/// are still running after the discovery failure), the outcome is [`Outcome::Stopped`]: the stop
/// is what ended the traversal. In every case `discovery.failed` in the report is the
/// authoritative record of the discovery failure, and the report keeps attempted work
/// (`unfinished`, `retained`), known but unattempted work (`queued`) and the undiscovered
/// remainder (`discovery.resume`) apart.
///
/// ```
/// use std::num::NonZeroUsize;
/// use concord_pagination::{manifest_jobs, Budget, JobOptions, Manifest, Options, Outcome, Resume};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// // Two manifests listing result URLs; each job downloads, parses and stores one result.
/// async fn manifest(key: Option<u32>) -> Result<Manifest<String, u32>, std::io::Error> {
///     Ok(match key {
///         None => Manifest::new(vec!["r/0".into(), "r/1".into()], Some(2)),
///         _ => Manifest::new(vec!["r/2".into()], None),
///     })
/// }
/// async fn download_parse_store(url: String) -> Result<usize, std::io::Error> {
///     Ok(url.len()) // a receipt: records written to this job's own staged shard
/// }
///
/// let n = |n| NonZeroUsize::new(n).unwrap();
/// let options = JobOptions::new(
///     Budget::new(n(10)),
///     Options::new(Budget::new(n(100))).window(n(2)),
/// );
/// let mut jobs = manifest_jobs(Resume::Start, options, manifest, download_parse_store);
/// let mut receipts = Vec::new();
/// while let Some(outcome) = jobs.next().await {
///     receipts.push(outcome.unwrap().value);
/// }
/// assert_eq!(receipts, [3, 3, 3]); // discovery order (the default)
/// let report = jobs.finish();
/// assert_eq!(report.outcome, Outcome::Exhausted);
/// assert_eq!(report.discovery.calls, 2);
/// # }
/// ```
pub fn manifest_jobs<D, MK, R, EJ, ED, FD, FutD, FJ, FutJ>(
    from: Resume<MK>,
    options: JobOptions,
    discover: FD,
    job: FJ,
) -> Jobs<D, MK, R, EJ, ED, FD, FutD, FJ, FutJ>
where
    D: Clone,
    MK: Clone,
    FD: FnMut(Option<MK>) -> FutD,
    FutD: Future<Output = Result<Manifest<D, MK>, ED>>,
    FJ: FnMut(D) -> FutJ,
    FutJ: Future<Output = Result<R, EJ>>,
{
    Jobs {
        discover,
        job,
        window: Window::new(&options.jobs),
        job_budget: options.jobs.budget.get() as u64,
        on_error: options.jobs.on_error,
        backlog: VecDeque::new(),
        backlog_cap: options.backlog.unwrap_or(options.jobs.window).get(),
        discovery_budget: options.discovery.get() as u64,
        position: Some(from),
        discovery: None,
        calls: 0,
        discovery_failed: None,
        discovery_repeated: false,
        history: None,
        ended: None,
        _types: PhantomData,
    }
}

/// An error returned by [`Jobs::next`].
///
/// `Debug` prints the job's index, never its descriptor; `Display` adds the underlying error's
/// own `Display`.
pub enum JobsError<D, EJ, ED> {
    /// A job failed: its descriptor, discovery index and error.
    Job(Keyed<D, EJ>),
    /// A manifest call failed; discovery is over (see [`DiscoveryReport::failed`]).
    Discovery(ED),
    /// A manifest continuation key was already requested in this traversal. The duplicate
    /// request was refused; inspect the traversal's progress and discovery resume point.
    RepeatedKey,
}

impl<D, EJ, ED> fmt::Debug for JobsError<D, EJ, ED> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Job(job) => f.debug_tuple("Job").field(job).finish(),
            Self::Discovery(_) => f.write_str("Discovery(..)"),
            Self::RepeatedKey => f.write_str("RepeatedKey"),
        }
    }
}

impl<D, EJ: fmt::Display, ED: fmt::Display> fmt::Display for JobsError<D, EJ, ED> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Job(job) => write!(f, "job {job}"),
            Self::Discovery(error) => write!(f, "discovery: {error}"),
            Self::RepeatedKey => f.write_str("a manifest continuation key was already requested"),
        }
    }
}

impl<D, EJ, ED> std::error::Error for JobsError<D, EJ, ED>
where
    EJ: std::error::Error + 'static,
    ED: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Job(job) => Some(&job.value),
            Self::Discovery(error) => Some(error),
            Self::RepeatedKey => None,
        }
    }
}

/// Manifest-discovered jobs; see [`manifest_jobs`].
///
/// [`next`](Self::next) is cancellation-safe; dropping the traversal drops the manifest call and
/// every job in flight (their partial sink output is the job's to clean up, typically in the
/// job's own drop guard), and nothing is admitted afterwards.
pub struct Jobs<D, MK, R, EJ, ED, FD, FutD, FJ, FutJ> {
    discover: FD,
    job: FJ,
    window: Window<D, R, EJ, FutJ>,
    job_budget: u64,
    on_error: OnError,
    backlog: VecDeque<D>,
    backlog_cap: usize,
    discovery_budget: u64,
    /// The next manifest to fetch; `None` once a manifest had no next key.
    position: Option<Resume<MK>>,
    discovery: Option<Pin<Box<FutD>>>,
    calls: u64,
    discovery_failed: Option<Resume<MK>>,
    discovery_repeated: bool,
    history: Option<Box<KeyHistory<MK>>>,
    ended: Option<Outcome>,
    _types: PhantomData<fn() -> ED>,
}

impl<D, MK, R, EJ, ED, FD, FutD, FJ, FutJ> Jobs<D, MK, R, EJ, ED, FD, FutD, FJ, FutJ>
where
    D: Clone,
    MK: Clone,
    FD: FnMut(Option<MK>) -> FutD,
    FutD: Future<Output = Result<Manifest<D, MK>, ED>>,
    FJ: FnMut(D) -> FutJ,
    FutJ: Future<Output = Result<R, EJ>>,
{
    /// Refuse a manifest continuation key already requested in this traversal, returning
    /// [`JobsError::RepeatedKey`] once before another discovery request. The refused key stays
    /// in the discovery checkpoint/report, with no failed manifest or additional call counted.
    ///
    /// Requires `Eq + Hash` and stores one cloned key per distinct keyed manifest requested;
    /// defaults have no history allocation, hashing or extra cloning. Enable before reading;
    /// only later admissions are remembered, and enabling again preserves history. Discovery
    /// and job budgets take precedence over refusal. A new traversal from [`Resume`] starts a
    /// fresh history; `Resume::Start` has no key to remember.
    ///
    /// As with a discovery failure, [`OnError::Stop`] cancels admitted jobs, while
    /// [`OnError::Continue`] finishes known admitted/queued work within its job budget. Once
    /// settled, the report is [`Outcome::DiscoveryRepeatedKey`]; finishing earlier with work
    /// outstanding is [`Outcome::Stopped`].
    pub fn reject_repeated_keys(mut self) -> Self
    where
        MK: Eq + Hash,
    {
        if self.history.is_none() {
            self.history = Some(Box::new(KeyHistory::new()));
        }
        self
    }

    /// The next keyed job outcome (or discovery failure), or `None` when the traversal ended.
    pub async fn next(&mut self) -> Option<Result<Keyed<D, R>, JobsError<D, EJ, ED>>> {
        poll_fn(|cx| self.poll_outcome(cx)).await
    }

    fn poll_outcome(&mut self, cx: &mut Context<'_>) -> Polled<D, R, EJ, ED> {
        loop {
            if self.ended.is_some() {
                return Poll::Ready(None);
            }
            if let Some(outcome) = self.window.take_ready() {
                return Poll::Ready(Some(outcome.map_err(JobsError::Job)));
            }
            self.admit_jobs();
            if self.admit_discovery() {
                return Poll::Ready(Some(Err(JobsError::RepeatedKey)));
            }
            if let Some(future) = self.discovery.as_mut()
                && let Poll::Ready(result) = future.as_mut().poll(cx)
            {
                self.discovery = None;
                match result {
                    Ok(manifest) => {
                        self.position = manifest.next.map(Resume::At);
                        self.backlog.extend(manifest.items);
                        continue;
                    }
                    Err(error) => {
                        // Terminal for discovery: the failed manifest is the resume point.
                        self.discovery_failed = self.position.clone();
                        if self.on_error == OnError::Stop {
                            self.window.halt(Halt::Stopped);
                        }
                        return Poll::Ready(Some(Err(JobsError::Discovery(error))));
                    }
                }
            }
            match self.window.poll_active(cx) {
                Step::Emit(outcome) => {
                    if self.window.halted().is_some() {
                        // A stopping job failure also drops the manifest call.
                        self.discovery = None;
                    }
                    return Poll::Ready(Some(outcome.map_err(JobsError::Job)));
                }
                Step::Again => {}
                Step::Pending => return Poll::Pending,
                Step::Idle if self.discovery.is_some() => return Poll::Pending,
                Step::Idle => {
                    self.ended = Some(self.natural_outcome());
                    return Poll::Ready(None);
                }
            }
        }
    }

    fn admit_jobs(&mut self) {
        while self.window.has_room() && self.window.admitted() < self.job_budget {
            let Some(descriptor) = self.backlog.pop_front() else {
                break;
            };
            let future = (self.job)(descriptor.clone());
            self.window.admit(descriptor, future);
        }
    }

    fn may_discover(&self) -> bool {
        self.discovery.is_none()
            && self.discovery_failed.is_none()
            && !self.discovery_repeated
            && self.window.halted().is_none()
            && self.position.is_some()
            && self.calls < self.discovery_budget
            && self.backlog.len() < self.backlog_cap
            && self.window.admitted() + (self.backlog.len() as u64) < self.job_budget
    }

    /// Returns true only on the first refused repeated key.
    fn admit_discovery(&mut self) -> bool {
        if !self.may_discover() {
            return false;
        }
        let Some(position) = &self.position else {
            return false;
        };
        if let Some(history) = &mut self.history
            && let Some(key) = position.key()
            && !history.admit(key)
        {
            self.discovery_repeated = true;
            if self.on_error == OnError::Stop {
                self.window.halt(Halt::Stopped);
            }
            return true;
        }
        self.calls += 1;
        let request = position.to_request();
        self.discovery = Some(Box::pin((self.discover)(request)));
        false
    }

    fn natural_outcome(&self) -> Outcome {
        match self.window.halted() {
            Some(Halt::Stopped) if self.discovery_failed.is_none() && !self.discovery_repeated => {
                return Outcome::Stopped;
            }
            _ if self.discovery_repeated => return Outcome::DiscoveryRepeatedKey,
            _ if self.discovery_failed.is_some() => return Outcome::DiscoveryFailed,
            Some(Halt::Failure) => return Outcome::Failed,
            Some(Halt::FailureLimit) => return Outcome::FailureLimit,
            _ => {}
        }
        if !self.backlog.is_empty() {
            Outcome::Bounded
        } else if self.position.is_some() {
            if self.calls >= self.discovery_budget {
                Outcome::DiscoveryBounded
            } else {
                Outcome::Bounded
            }
        } else if self.window.failures() > 0 {
            Outcome::ExhaustedWithFailures
        } else {
            Outcome::Exhausted
        }
    }

    fn can_admit(&self) -> bool {
        let job = self.window.halted().is_none()
            && !self.backlog.is_empty()
            && self.window.admitted() < self.job_budget;
        job || self.may_discover()
    }

    /// Counters of the jobs and of discovery.
    pub fn progress(&self) -> JobsProgress {
        JobsProgress {
            jobs: self.window.progress(),
            discovery_calls: self.calls,
            discovery_in_flight: self.discovery.is_some(),
            queued: self.backlog.len(),
        }
    }

    /// A snapshot: sparse job progress, queued descriptors and the discovery position. Clones
    /// keys and descriptors.
    pub fn checkpoint(&self) -> JobsCheckpoint<D, MK> {
        JobsCheckpoint {
            jobs: self.window.checkpoint(),
            queued: self.backlog.iter().cloned().collect(),
            discovery: self.position.clone(),
        }
    }

    /// Ends the traversal and reports how. The manifest call and the jobs in flight are dropped
    /// (outcome [`Outcome::Stopped`] unless the traversal had already ended — also after a
    /// discovery failure whose known jobs were still running; `discovery.failed` keeps that
    /// failure).
    pub fn finish(mut self) -> JobsReport<D, MK, R, EJ> {
        let outcome = match self.ended {
            Some(outcome) => outcome,
            None if self.window.is_settled() && self.discovery.is_none() && !self.can_admit() => {
                self.natural_outcome()
            }
            None => {
                self.window.halt(Halt::Stopped);
                self.discovery = None;
                Outcome::Stopped
            }
        };
        let progress = JobsProgress {
            discovery_in_flight: false,
            ..self.progress()
        };
        let settled = self.window.settle();
        JobsReport {
            outcome,
            progress,
            unfinished: settled.unfinished,
            retained: settled.retained,
            unemitted_errors: settled.unemitted_errors,
            queued: self.backlog.into_iter().collect(),
            discovery: DiscoveryReport {
                calls: self.calls,
                resume: self.position,
                failed: self.discovery_failed,
                repeated_key: self.discovery_repeated,
            },
        }
    }
}

impl<D, MK, R, EJ, ED, FD, FutD, FJ, FutJ> fmt::Debug for Jobs<D, MK, R, EJ, ED, FD, FutD, FJ, FutJ>
where
    D: Clone,
    FutJ: Future<Output = Result<R, EJ>>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Jobs")
            .field("jobs", &self.window.progress())
            .field("discovery_calls", &self.calls)
            .field("queued", &self.backlog.len())
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

/// Counters of a [`Jobs`] traversal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct JobsProgress {
    /// The job window's counters.
    pub jobs: Progress,
    /// Manifest calls so far (what the discovery budget has been charged).
    pub discovery_calls: u64,
    /// Whether a manifest call is in flight.
    pub discovery_in_flight: bool,
    /// Descriptors discovered and waiting for a job slot.
    pub queued: usize,
}

/// A snapshot of a [`Jobs`] traversal ([`Jobs::checkpoint`]).
#[derive(Clone)]
#[non_exhaustive]
pub struct JobsCheckpoint<D, MK> {
    /// Admitted jobs' sparse progress (descriptors in discovery order).
    pub jobs: Checkpoint<D>,
    /// Discovered descriptors not admitted yet, in discovery order.
    pub queued: Vec<D>,
    /// The next manifest to fetch; `None` once the chain ended.
    pub discovery: Option<Resume<MK>>,
}

impl<D, MK> fmt::Debug for JobsCheckpoint<D, MK> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobsCheckpoint")
            .field("jobs", &self.jobs)
            .field("queued", &self.queued.len())
            .field("discovery", &self.discovery)
            .finish()
    }
}

/// The discovery side of a [`JobsReport`].
#[non_exhaustive]
pub struct DiscoveryReport<MK> {
    /// Manifest calls made (empty manifests included).
    pub calls: u64,
    /// Where discovery would continue: `None` when the manifest chain ended (every descriptor was
    /// discovered); otherwise work may remain undiscovered from here, in unknown amount. Resume
    /// with [`manifest_jobs`] from this point.
    pub resume: Option<Resume<MK>>,
    /// The manifest whose call failed, kept apart from `resume`. A locally refused repeated
    /// key is not a failed manifest call.
    pub failed: Option<Resume<MK>>,
    /// Whether a repeated continuation key stopped discovery, including when the caller
    /// subsequently stopped before known jobs settled. The refused key is in `resume`.
    pub repeated_key: bool,
}

impl<MK> fmt::Debug for DiscoveryReport<MK> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveryReport")
            .field("calls", &self.calls)
            .field("resume", &self.resume)
            .field("failed", &self.failed)
            .field("repeated_key", &self.repeated_key)
            .finish()
    }
}

/// How a [`Jobs`] traversal ended ([`Jobs::finish`]).
///
/// Outcomes: [`Outcome::Exhausted`], [`Outcome::ExhaustedWithFailures`], [`Outcome::Bounded`] (job
/// budget), [`Outcome::Failed`], [`Outcome::FailureLimit`], [`Outcome::Stopped`],
/// [`Outcome::DiscoveryFailed`], [`Outcome::DiscoveryBounded`] and
/// [`Outcome::DiscoveryRepeatedKey`].
///
/// Work is reported in three disjoint parts: attempted work (`unfinished`, `retained`, and the
/// successes already emitted), known but unattempted work (`queued`), and undiscovered work
/// (from `discovery.resume`, unknown in amount). A completed job whose receipt was retained for
/// ordered emission is in `retained`: its sink effects exist, and a restart point alone would hide
/// them.
#[non_exhaustive]
pub struct JobsReport<D, MK, R, EJ> {
    /// How the traversal ended.
    pub outcome: Outcome,
    /// Final counters.
    pub progress: JobsProgress,
    /// Admitted jobs without a successful emitted outcome, in discovery order (failed, cancelled,
    /// retained).
    pub unfinished: Vec<Keyed<D, Unfinished>>,
    /// Completed jobs whose receipts were never emitted, in discovery order.
    pub retained: Vec<Keyed<D, R>>,
    /// Errors of failed jobs that were never emitted, in discovery order.
    pub unemitted_errors: Vec<Keyed<D, EJ>>,
    /// Descriptors discovered but never admitted, in discovery order.
    pub queued: Vec<D>,
    /// Discovery's state.
    pub discovery: DiscoveryReport<MK>,
}

impl<D, MK, R, EJ> JobsReport<D, MK, R, EJ> {
    /// `true` only when every descriptor was discovered and every job succeeded.
    pub fn is_success(&self) -> bool {
        self.outcome.is_success()
    }

    /// The first descriptor in discovery order without a successful emitted outcome (the ordered
    /// restart point), else the first queued descriptor.
    pub fn restart(&self) -> Option<&D> {
        self.unfinished
            .first()
            .map(|entry| &entry.key)
            .or_else(|| self.queued.first())
    }

    /// Failed descriptors, in discovery order.
    ///
    /// These are the descriptors the jobs *started* with. A job with internal progress (several
    /// pages, a partly written shard) must carry its own restart point in its error: rerunning
    /// the descriptor repeats the work done before the failure.
    pub fn failed(&self) -> impl Iterator<Item = &D> {
        self.unfinished
            .iter()
            .filter(|entry| entry.value == Unfinished::Failed)
            .map(|entry| &entry.key)
    }

    /// Decomposes the report, without losing anything, into what to keep, the known work to run
    /// again and the undiscovered work; see [`JobsResume`].
    ///
    /// ```
    /// use std::future::ready;
    /// use std::num::NonZeroUsize;
    /// use concord_pagination::{manifest_jobs, Budget, JobOptions, Manifest, Options, Outcome};
    /// use concord_pagination::{Resume, Unfinished};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let n = |n| NonZeroUsize::new(n).unwrap();
    /// // "b" fails and stops the traversal: "c" stays queued, the second manifest undiscovered.
    /// let options = JobOptions::new(Budget::new(n(10)), Options::new(Budget::new(n(10))));
    /// let mut jobs = manifest_jobs(
    ///     Resume::Start,
    ///     options,
    ///     |key: Option<u32>| {
    ///         ready(Ok::<_, std::io::Error>(match key {
    ///             None => Manifest::new(vec!["a", "b", "c"], Some(1)),
    ///             _ => Manifest::new(vec!["d"], None),
    ///         }))
    ///     },
    ///     |url: &'static str| ready(if url == "b" { Err("b broke") } else { Ok(url.len()) }),
    /// );
    /// while let Some(outcome) = jobs.next().await {
    ///     let _ = outcome; // "a" succeeds, then "b" fails
    /// }
    /// let resume = jobs.finish().into_resume();
    /// assert_eq!(resume.outcome, Outcome::Failed);
    /// assert_eq!(resume.progress.jobs.admitted, 2);
    /// let rerun: Vec<_> = resume.rerun.iter().map(|job| (job.key, job.value)).collect();
    /// assert_eq!(rerun, [("b", Unfinished::Failed)]); // attempted, to run again
    /// assert_eq!(resume.queued, ["c"]); // known, never attempted
    /// assert_eq!(resume.undiscovered(), Some(&Resume::At(1))); // unknown in amount
    /// assert_eq!(resume.known().copied().collect::<Vec<_>>(), ["b", "c"]);
    /// # }
    /// ```
    pub fn into_resume(self) -> JobsResume<D, MK, R, EJ> {
        // A retained entry is represented once, by its receipt in `retained`.
        let rerun = self
            .unfinished
            .into_iter()
            .filter(|entry| entry.value != Unfinished::Retained)
            .collect();
        JobsResume {
            outcome: self.outcome,
            progress: self.progress,
            retained: self.retained,
            unemitted_errors: self.unemitted_errors,
            rerun,
            queued: self.queued,
            discovery: self.discovery,
        }
    }

    /// Splits the report into retained receipts and the descriptors still to run (unfinished and
    /// not retained, then queued). Run those with [`independent`](crate::independent), and
    /// continue discovery from `discovery.resume` with [`manifest_jobs`]. Like
    /// [`failed`](Self::failed), these are start descriptors: a failed job with internal progress
    /// restarts from the beginning unless its error carried a later restart point that you use
    /// instead.
    ///
    /// This split is lossy: it drops the outcome, the counters, each descriptor's classification
    /// (failed, cancelled or queued), the unemitted errors and the discovery state (calls,
    /// resume and failed manifest). [`into_resume`](Self::into_resume) keeps all of them.
    pub fn into_rerun(self) -> (Vec<Keyed<D, R>>, Vec<D>) {
        let mut descriptors: Vec<D> = self
            .unfinished
            .into_iter()
            .filter(|entry| entry.value != Unfinished::Retained)
            .map(|entry| entry.key)
            .collect();
        descriptors.extend(self.queued);
        (self.retained, descriptors)
    }
}

/// A [`JobsReport`] decomposed without loss ([`JobsReport::into_resume`]): what to keep, the
/// known work to run again, and the undiscovered work.
///
/// Every part of the report is here, moved, never cloned (receipts and errors need no `Clone`):
/// the outcome, the counters, the retained receipts, the unemitted errors, every unfinished
/// entry with its classification, the queued descriptors and the whole [`DiscoveryReport`]. The
/// report's [`Unfinished::Retained`] entries are not repeated in `rerun`: each is the `retained`
/// entry with the same index and descriptor (one per retained entry, as [`Jobs::finish`]
/// returns them).
///
/// The work left is in three disjoint parts:
///
/// - **keep**: `retained`, completed jobs whose sink effects exist; do not run them again;
/// - **known work** ([`known`](Self::known)): `rerun` (attempted: failed or cancelled), then
///   `queued` (discovered, never attempted), in discovery order; run it with
///   [`independent`](crate::independent);
/// - **undiscovered work** ([`undiscovered`](Self::undiscovered)): from `discovery.resume`,
///   unknown in amount; continue it with [`manifest_jobs`].
///
/// As with [`JobsReport::failed`], descriptors are start descriptors: a failed job with internal
/// progress restarts from the beginning unless its error (in `unemitted_errors`, or the one
/// `next` returned) carried a later restart point.
#[non_exhaustive]
pub struct JobsResume<D, MK, R, EJ> {
    /// How the traversal ended.
    pub outcome: Outcome,
    /// Final counters, as in the report.
    pub progress: JobsProgress,
    /// Completed jobs whose receipts were never emitted, in discovery order: keep them.
    pub retained: Vec<Keyed<D, R>>,
    /// Errors of failed jobs that were never emitted, in discovery order. Their descriptors are
    /// in `rerun` as [`Unfinished::Failed`].
    pub unemitted_errors: Vec<Keyed<D, EJ>>,
    /// Attempted jobs to run again, with their discovery index and classification
    /// ([`Unfinished::Failed`] or [`Unfinished::Cancelled`]), in discovery order: the report's
    /// unfinished entries except the retained ones.
    pub rerun: Vec<Keyed<D, Unfinished>>,
    /// Discovered descriptors never admitted, in discovery order (after every `rerun` entry).
    pub queued: Vec<D>,
    /// Discovery's state: manifest calls, the undiscovered remainder (`resume`) and the
    /// manifest whose call failed (`failed`).
    pub discovery: DiscoveryReport<MK>,
}

impl<D, MK, R, EJ> JobsResume<D, MK, R, EJ> {
    /// The known work still to run, in discovery order: the `rerun` descriptors, then `queued`.
    /// Retained jobs are not in it.
    pub fn known(&self) -> impl Iterator<Item = &D> {
        self.rerun
            .iter()
            .map(|entry| &entry.key)
            .chain(self.queued.iter())
    }

    /// Where undiscovered work starts (`discovery.resume`): `None` when every manifest was
    /// discovered.
    pub fn undiscovered(&self) -> Option<&Resume<MK>> {
        self.discovery.resume.as_ref()
    }
}

impl<D, MK, R, EJ> fmt::Debug for JobsResume<D, MK, R, EJ> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobsResume")
            .field("outcome", &self.outcome)
            .field("progress", &self.progress)
            .field("retained", &self.retained.len())
            .field("unemitted_errors", &self.unemitted_errors.len())
            .field("rerun", &self.rerun.len())
            .field("queued", &self.queued.len())
            .field("discovery", &self.discovery)
            .finish()
    }
}

impl<D, MK, R, EJ> fmt::Debug for JobsReport<D, MK, R, EJ> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobsReport")
            .field("outcome", &self.outcome)
            .field("progress", &self.progress)
            .field("unfinished", &self.unfinished.len())
            .field("retained", &self.retained.len())
            .field("unemitted_errors", &self.unemitted_errors.len())
            .field("queued", &self.queued.len())
            .field("discovery", &self.discovery)
            .finish()
    }
}
