//! The updated consumer (stage 3 of the maintenance exercise), beside the frozen one.
//!
//! The mailing tool now takes tab-separated files with CRLF line endings, and must not mail
//! Contacts who opted out of e-mail. So the export switches its job dialect from comma / LF to
//! [`MAILING_DIALECT`] (`TAB` / `CRLF`), and uses the column the SDK added to its Contact export,
//! `HasOptedOutOfEmail` ([`ContactExportRow::has_opted_out_of_email`]).
//!
//! The pipeline itself is the frozen consumer's ([`before::export_contacts`]): describe validation,
//! the select list, the job and the typed result sets all follow the SDK's export schema and the
//! job's dialect, so none of it needed an edit. This module adds the opt-out rule and the mailing
//! file, written with the SDK's dialect adapter in the job's dialect (no CSV code here).

use std::future::Future;
use std::num::NonZeroUsize;

use concord::csv::Rows;
use concord_example_salesforce_sdk::OrgContext;
use concord_example_salesforce_sdk::bulk::{BulkDialect, ColumnDelimiter, LineEnding};
use concord_example_salesforce_sdk::csv::{
    CONTACT_EXPORT_FIELDS, ContactExportRow, CsvError, FieldColumns, SalesforceCsv,
};
use concord_example_salesforce_sdk::ids::{QueryJobId, RecordId};
use concord_pagination::{Budget, Outcome};

use crate::before::{self, DescribedColumn, MailingError, MailingExport};

/// The mailing tool's dialect: tab-separated, CRLF.
pub const MAILING_DIALECT: BulkDialect = BulkDialect::new(ColumnDelimiter::Tab, LineEnding::Crlf);

/// The deployment's export after the change: [`MAILING_DIALECT`], no filter, no limit.
pub fn mailing_export(set_budget: Budget, max_polls: NonZeroUsize) -> MailingExport {
    let mut export = MailingExport::new(set_budget, max_polls);
    export.dialect = MAILING_DIALECT;
    export
}

/// Who gets mailed, and who does not.
#[derive(Debug)]
pub struct MailingList {
    /// The query job.
    pub job: QueryJobId,
    /// The job's dialect: results were read, and [`to_csv`](Self::to_csv) writes, in it.
    pub dialect: BulkDialect,
    /// The export columns as the org described them.
    pub columns: Vec<DescribedColumn>,
    /// Contacts with an e-mail address who did not opt out, in result order.
    pub recipients: Vec<ContactExportRow>,
    /// Contacts who opted out of e-mail (`HasOptedOutOfEmail = true`).
    pub opted_out: Vec<RecordId>,
    /// Contacts without an e-mail address who did not opt out.
    pub without_email: Vec<RecordId>,
    /// `Exhausted`, or `Bounded` by the result-set budget.
    pub outcome: Outcome,
}

impl MailingList {
    /// The mailing file: the export header and the recipients, in the job's dialect.
    ///
    /// # Errors
    ///
    /// [`CsvError`].
    pub fn to_csv(&self) -> Result<Vec<u8>, CsvError> {
        let columns = FieldColumns::new(CONTACT_EXPORT_FIELDS)?;
        let writer = SalesforceCsv::new(self.dialect).writer(&columns)?;
        Ok(writer.to_vec(Rows::new(self.recipients.iter()))?)
    }
}

/// Runs the export (**creates a Bulk query job in the org**) and applies the opt-out rule.
///
/// # Errors
///
/// [`MailingError`], as [`before::export_contacts`].
pub async fn mailing_list<W, F>(
    ctx: &OrgContext,
    export: &MailingExport,
    wait: W,
) -> Result<MailingList, MailingError>
where
    W: FnMut(u32) -> F,
    F: Future<Output = ()>,
{
    let exported = before::export_contacts(ctx, export, wait).await?;
    let mut recipients = Vec::new();
    let mut opted_out = Vec::new();
    let mut without_email = Vec::new();
    for row in exported.rows {
        if row.has_opted_out_of_email {
            opted_out.push(row.id);
        } else if row.email.is_none() {
            without_email.push(row.id);
        } else {
            recipients.push(row);
        }
    }
    Ok(MailingList {
        job: exported.job,
        dialect: exported.dialect,
        columns: exported.columns,
        recipients,
        opted_out,
        without_email,
        outcome: exported.outcome,
    })
}
