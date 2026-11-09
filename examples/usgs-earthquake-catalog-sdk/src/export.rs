use crate::{CsvRow, CsvRows, Error, PageSize, Response, UsgsCatalogWindow};
use concord_codecs::stream::Completion;
use concord_pagination::{OffsetError, next_offset};
use std::{
    future::Future,
    num::{NonZeroU64, NonZeroUsize},
    pin::Pin,
};

type Opening = Pin<Box<dyn Future<Output = Result<Response<CsvRows>, Error>> + Send>>;

/// A bounded sequence of incremental CSV responses. Owns at most one response reader;
/// rows belong to the caller as soon as returned. No tasks or prefetch are started.
#[derive(concord::Debug)]
#[sensitive]
pub struct CsvExport {
    query: UsgsCatalogWindow,
    size: u16,
    offset: NonZeroU64,
    budget: usize,
    #[public]
    pages: usize,
    rows_in_page: u64,
    #[public]
    delivered: u64,
    reader: Option<CsvRows>,
    opening: Option<Opening>,
    #[public]
    completion: Completion,
}

impl CsvExport {
    pub(crate) fn new(
        query: UsgsCatalogWindow,
        size: PageSize,
        offset: NonZeroU64,
        budget: NonZeroUsize,
    ) -> Self {
        Self {
            query,
            size: size.get(),
            offset,
            budget: budget.get(),
            pages: 0,
            rows_in_page: 0,
            delivered: 0,
            reader: None,
            opening: None,
            completion: Completion::Reading,
        }
    }

    /// Read the next row. Every bad row is terminal; later calls return `ExportFailed`.
    /// A successful `None` means every response reached clean EOF and the catalog exhausted.
    /// Earlier rows remain delivered on every failure. Dropping the export cancels local work.
    pub async fn next(&mut self) -> Result<Option<CsvRow>, Error> {
        match self.completion {
            Completion::Failed => return Err(Error::ExportFailed),
            Completion::EndOfInput => return Ok(None),
            _ => {}
        }
        let result = self.read_next().await;
        if result.is_err() {
            self.completion = Completion::Failed;
            self.reader = None;
            self.opening = None;
        }
        result
    }

    async fn read_next(&mut self) -> Result<Option<CsvRow>, Error> {
        loop {
            if let Some(reader) = &mut self.reader {
                match reader.next().await {
                    Some(Ok(row)) => {
                        if self.rows_in_page >= u64::from(self.size) {
                            return Err(Error::Protocol("CSV page exceeds requested limit"));
                        }
                        self.rows_in_page += 1;
                        self.delivered += 1;
                        return Ok(Some(row));
                    }
                    Some(Err(error)) => return Err(error),
                    None => {
                        // The shared reader signals an error before it becomes fused.
                        if reader.completion() != Completion::EndOfInput {
                            return Err(Error::Protocol("CSV response did not complete"));
                        }
                        self.reader = None;
                        let next = next_offset(
                            self.offset.get(),
                            u64::from(self.size),
                            self.rows_in_page,
                            None,
                        )
                        .map_err(|error| match error {
                            OffsetError::Overflow => Error::Protocol("catalog offset overflow"),
                            _ => Error::Protocol("CSV page exceeds requested limit"),
                        })?;
                        let Some(next) = next.and_then(NonZeroU64::new) else {
                            self.completion = Completion::EndOfInput;
                            return Ok(None);
                        };
                        self.offset = next;
                        self.rows_in_page = 0;
                    }
                }
            }
            if self.opening.is_none() {
                if self.pages == self.budget {
                    return Err(Error::PageBudget {
                        next_offset: self.offset,
                    });
                }
                self.pages += 1;
                let request = self.query.csv().set_offset(self.offset);
                self.opening = Some(Box::pin(request.send()));
            }
            let opened = self
                .opening
                .as_mut()
                .expect("opening was initialized")
                .await;
            self.opening = None;
            match opened {
                Ok(response) => self.reader = Some(response.data),
                Err(error) if error.is_no_data() => {
                    self.completion = Completion::EndOfInput;
                    return Ok(None);
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Reading, failed, or exhausted. Only `EndOfInput` establishes a complete traversal.
    pub fn completion(&self) -> Completion {
        self.completion
    }
    /// Rows already handed to the caller, including before a failure.
    pub fn rows_delivered(&self) -> u64 {
        self.delivered
    }
    /// Logical page requests admitted; no retry is performed by this SDK.
    pub fn pages_requested(&self) -> usize {
        self.pages
    }
}
