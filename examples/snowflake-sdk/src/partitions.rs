//! Discovery from a validated first result, then independently indexed bounded fanout.
use crate::model::PartitionBody;
use crate::{Error, ProtocolError, Query, Reply, ResultMetadata, ResultSet, Row};
use concord::ResponseMeta;
use concord_pagination::{Independent, IndependentReport, Options, independent};
use std::{future::Future, num::NonZeroUsize, ops::Range, pin::Pin, sync::Arc};

/// A validated first response. Its rows are owned here and never refetched by traversal.
#[derive(Debug)]
pub struct FirstResult {
    query: Query,
    response: Reply<ResultSet>,
}
impl FirstResult {
    pub(crate) fn new(query: Query, response: Reply<ResultSet>) -> Result<Self, Error> {
        let r = &response.value;
        let m = &r.result_set_meta_data;
        if r.status.code == "391908" {
            return Err(Error::protocol(
                ProtocolError::IncompleteFirstPartition,
                Some(&response.meta),
            ));
        }
        if !["0", "090001"].contains(&r.status.code.as_str())
            || r.status
                .sql_state
                .as_deref()
                .is_some_and(|s| !s.is_empty() && s != "00000")
            || r.status.extra.contains_key("statementHandles")
            || m.format != "jsonv2"
            || m.partition_info.is_empty()
            || m.extra
                .get("partition")
                .is_some_and(|p| p.as_u64() != Some(0))
            || m.partition_info
                .iter()
                .try_fold(0u64, |n, p| n.checked_add(p.row_count))
                != Some(m.num_rows)
        {
            return Err(Error::protocol(
                ProtocolError::InvalidManifest,
                Some(&response.meta),
            ));
        }
        validate_rows(&r.data, m, 0, &response.meta)?;
        Ok(Self { query, response })
    }
    /// Original result, including partition zero, all descriptors and response headers.
    pub fn response(&self) -> &Reply<ResultSet> {
        &self.response
    }
    /// A plan bound to this account, handle, request context and server descriptors.
    /// The proof is partitionInfo's ordered descriptors plus the documented indexed GET contract,
    /// not numRows or an inferred page size. No server URL is followed.
    pub fn plan(&self) -> PartitionPlan {
        PartitionPlan {
            query: self.query.clone(),
            metadata: Arc::new(self.response.value.result_set_meta_data.clone()),
        }
    }
    /// Yields partition zero first, without I/O, then uses Concord's independent scheduler.
    /// The budget counts only later HTTP partition requests; the first response already exists.
    pub fn partitions(self, options: Options) -> Partitions {
        let plan = self.plan();
        let count = plan.metadata.partition_info.len();
        let first = Partition {
            index: 0,
            rows: self.response.value.data,
            meta: self.response.meta,
            plan: plan.clone(),
        };
        let fetch_plan = plan.clone();
        let fetch: Fetch = Box::new(move |index| {
            let plan = fetch_plan.clone();
            Box::pin(async move { plan.fetch(index).await })
        });
        Partitions {
            first: Some(first),
            engine: independent(1..count, options, fetch),
            plan,
        }
    }
    /// Convenience options: finite request budget and nonzero concurrency.
    pub fn bounded_partitions(
        self,
        max_requests: NonZeroUsize,
        concurrency: NonZeroUsize,
    ) -> Partitions {
        self.partitions(Options::new(max_requests.into()).window(concurrency))
    }
}

/// Immutable independent partition plan discovered from a successful first response.
#[derive(Clone, Debug)]
pub struct PartitionPlan {
    query: Query,
    metadata: Arc<ResultMetadata>,
}
impl PartitionPlan {
    /// Query identity and original request context.
    pub fn query(&self) -> &Query {
        &self.query
    }
    /// Exact server descriptors; partition sizes are never recomputed as equal-sized pages.
    pub fn metadata(&self) -> &ResultMetadata {
        &self.metadata
    }
    /// Fetches any later partition independently (e.g. 2 before 1). Index zero is already owned
    /// by FirstResult. Rows and any echoed handle are validated before returning the partition.
    pub async fn fetch(&self, index: usize) -> Result<Partition, Error> {
        if index == 0 || index >= self.metadata.partition_info.len() {
            return Err(Error::protocol(ProtocolError::InvalidPartition, None));
        }
        let response = self
            .query
            .client
            .execute(
                self.query
                    .client
                    .api
                    .statement(self.query.handle.clone())
                    .partition(index)
                    .prepare()?,
                &[200],
                Some(&self.query.handle),
            )
            .await?;
        let body = response
            .value
            .decode(self.query.client.options.max_response_bytes)
            .map_err(|source| Error::PartitionBody {
                meta: Box::new(response.meta.clone()),
                source,
            })?;
        let rows = match body {
            PartitionBody::Envelope {
                data,
                statement_handle,
            } => {
                if let Some(handle) = &statement_handle {
                    self.query.correlate(Some(handle), &response.meta)?;
                }
                data
            }
            PartitionBody::Rows(rows) => rows,
        };
        validate_rows(&rows, &self.metadata, index, &response.meta)?;
        Ok(Partition {
            index,
            rows,
            meta: response.meta,
            plan: self.clone(),
        })
    }
}
fn validate_rows(
    rows: &[Row],
    metadata: &ResultMetadata,
    index: usize,
    meta: &ResponseMeta,
) -> Result<(), Error> {
    if rows.len() as u64 != metadata.partition_info[index].row_count
        || rows.iter().any(|row| row.len() != metadata.row_type.len())
    {
        return Err(Error::protocol(ProtocolError::InvalidRows, Some(meta)));
    }
    Ok(())
}

/// A whole validated partition, delivered in plan order by default.
/// Memory is bounded per HTTP body; this is partition-level delivery, not a row byte stream.
pub struct Partition {
    /// Index from partitionInfo (not the scheduler's relative index).
    pub index: usize,
    /// String/null SQL rows.
    pub rows: Vec<Row>,
    /// Response head for this partition; partition zero keeps the original POST/status head.
    pub meta: ResponseMeta,
    plan: PartitionPlan,
}
impl Partition {
    /// Metadata and query correlation retained even after the traversal is dropped.
    pub fn plan(&self) -> &PartitionPlan {
        &self.plan
    }
}
impl std::fmt::Debug for Partition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Partition")
            .field("index", &self.index)
            .field("row_count", &self.rows.len())
            .finish_non_exhaustive()
    }
}

/// Failure explicitly correlated to its requested partition.
#[derive(Debug)]
pub struct PartitionFailure {
    /// Index that failed, independent of completion ordering.
    pub index: usize,
    /// Typed transport, protocol or server failure.
    pub error: Error,
}
type FetchFuture = Pin<Box<dyn Future<Output = Result<Partition, Error>> + Send>>;
type Fetch = Box<dyn FnMut(usize) -> FetchFuture + Send>;
type Engine = Independent<usize, Range<usize>, Partition, Error, Fetch, FetchFuture>;

/// Bounded, cancellation-safe partition traversal. Drop/finish stops local work only.
/// Always inspect finish(): None from next() alone is not a completeness signal.
pub struct Partitions {
    first: Option<Partition>,
    engine: Engine,
    plan: PartitionPlan,
}
impl Partitions {
    /// Gets the next partition. A dropped pending call leaves scheduler work owned by self.
    pub async fn next(&mut self) -> Option<Result<Partition, PartitionFailure>> {
        if let Some(first) = self.first.take() {
            return Some(Ok(first));
        }
        self.engine.next().await.map(|r| {
            r.map(|keyed| keyed.value)
                .map_err(|keyed| PartitionFailure {
                    index: keyed.key,
                    error: keyed.value,
                })
        })
    }
    /// Stops admission, drops local in-flight requests, and returns lossless progress evidence.
    pub fn finish(self) -> PartitionReport {
        PartitionReport {
            unconsumed_first: self.first,
            remaining: self.engine.finish(),
            plan: self.plan,
        }
    }
}

/// Completion evidence and work not delivered. Emitted partitions/errors belong to the caller.
#[derive(Debug)]
pub struct PartitionReport {
    /// Partition zero if the caller stopped before consuming it (never lost).
    pub unconsumed_first: Option<Partition>,
    /// Original Concord report: outcome, failures, cancelled work, retained results, unstarted range.
    pub remaining: IndependentReport<usize, Range<usize>, Partition, Error>,
    /// Bound query/context/metadata, usable for deliberate independent recovery.
    pub plan: PartitionPlan,
}
impl PartitionReport {
    /// True only after zero was delivered and every later partition was successfully emitted.
    pub fn is_complete(&self) -> bool {
        self.unconsumed_first.is_none() && self.remaining.is_success()
    }
    /// All indexes not successfully delivered, including retained results available in the report.
    /// No allocation is proportional to the number of partitions.
    pub fn missing(&self) -> impl Iterator<Item = usize> + '_ {
        self.unconsumed_first
            .iter()
            .map(|_| 0)
            .chain(self.remaining.unfinished.iter().map(|k| k.key))
            .chain(self.remaining.rest.clone().into_iter().flatten())
    }
}
