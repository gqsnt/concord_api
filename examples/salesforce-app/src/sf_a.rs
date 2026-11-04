//! SF-A — schema-aware CRM extraction.
//!
//! Input: an org context (pinned API version), typed filters and a page budget. Output: an
//! [`ExtractionReport`] with the retrieved Accounts and Contacts, the schema provenance of the
//! selected fields (from sObject Describe), per-page metadata and a completion state per section
//! that is either complete, bounded with a resume point, interrupted with a resume point, or an
//! explicit restart requirement.
//!
//! # Steps
//!
//! 1. **Schema.** Describe Account and Contact. Required selected fields must exist; optional
//!    standard and custom fields (`Customer_Tier__c`, `Loyalty_Tier__c`) are selected only when
//!    the org has them, and the report says which were missing. A relationship path
//!    (`Account.Name`) is selected when `AccountId` has a relationship name; it stays dynamic in
//!    the typed record.
//! 2. **Queries.** SOQL is built from validated identifiers and escaped literals
//!    ([`concord_example_salesforce_sdk::soql`]); the typed filters are user data and only ever
//!    enter as literals. Concord encodes the statement as the `q` query value.
//! 3. **Traversal.** Each section follows `nextRecordsUrl` locators through
//!    `concord_pagination::sequential` with the SDK's [`QueryPages`] factory. `totalSize` is
//!    reported, never used to guess a locator.
//! 4. **Record reads.** The requested ids are read through sObject Rows with the selected fields.
//!
//! # Failure after earlier success
//!
//! A section that fails after some pages keeps the records it has and reports why:
//!
//! - an expired or invalid query locator (`INVALID_QUERY_LOCATOR`) cannot be resumed:
//!   [`Completion::RestartRequired`]. The collected records are a prefix of a result set that
//!   no longer exists; a restart starts from the first batch, so a caller that keeps both sets
//!   must deduplicate by `Id` (the result can also have changed in between);
//! - any other failure (a 5xx, a transport error) keeps the failed batch's locator:
//!   [`Completion::Interrupted`] with a [`QueryResume`] to pass back in
//!   [`ExtractionInput::resume`]. Resuming re-fetches that batch, so nothing is duplicated or
//!   skipped *if* the locator is still valid; if it expired meanwhile, the resumed run reports
//!   `RestartRequired`.
//!
//! A [`QueryResume`] is bound to its org, API version and statement: resuming it against another
//! is refused ([`ExtractionError::ResumeMismatch`]).

use std::fmt;

use concord_example_salesforce_sdk::ids::{
    ApiVersion, BatchSize, FieldList, FieldName, QueryLocator, RecordId, SObjectName,
};
use concord_example_salesforce_sdk::model::{Account, Contact, SObjectDescribe};
use concord_example_salesforce_sdk::query::{QueryPageMeta, QueryPages, QueryRecord};
use concord_example_salesforce_sdk::soql::{Condition, Direction, FieldPath, Literal, Op, Soql};
use concord_example_salesforce_sdk::{OrgContext, SfError};
use concord_pagination::{Budget, Outcome, Resume, sequential};

/// The workflow's input.
#[derive(Clone, Debug)]
pub struct ExtractionInput {
    /// Only Accounts of this industry (`Industry = '..'`), when set.
    pub industry: Option<String>,
    /// Only Contacts whose e-mail ends with this text (`Email LIKE '%..'`), when set.
    pub email_suffix: Option<String>,
    /// The page budget of each section.
    pub page_budget: Budget,
    /// `Sforce-Query-Options: batchSize`.
    pub batch_size: Option<BatchSize>,
    /// Resume points from an earlier report.
    pub resume: ExtractionResume,
    /// Account ids to read individually (sObject Rows).
    pub account_reads: Vec<RecordId>,
}

impl ExtractionInput {
    /// No filters, no resume, no record reads.
    pub fn new(page_budget: Budget) -> Self {
        Self {
            industry: None,
            email_suffix: None,
            page_budget,
            batch_size: None,
            resume: ExtractionResume::default(),
            account_reads: Vec::new(),
        }
    }
}

/// Resume points per section.
#[derive(Clone, Debug, Default)]
pub struct ExtractionResume {
    /// The Account section's.
    pub accounts: Option<QueryResume>,
    /// The Contact section's.
    pub contacts: Option<QueryResume>,
}

/// Where a section resumes: bound to its org, API version and SOQL statement. `Debug` shows
/// neither the statement nor the locator.
#[derive(Clone, PartialEq, Eq)]
pub struct QueryResume {
    instance: String,
    version: ApiVersion,
    soql: String,
    at: Resume<QueryLocator>,
}

impl QueryResume {
    /// The resume position (`Start` or the locator of the first batch not received).
    pub fn position(&self) -> &Resume<QueryLocator> {
        &self.at
    }
}

impl fmt::Debug for QueryResume {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QueryResume")
            .field("version", &self.version)
            .field("at", &self.at)
            .finish_non_exhaustive()
    }
}

/// The workflow's result.
#[derive(Debug)]
pub struct ExtractionReport {
    /// Account schema provenance.
    pub account_schema: SchemaProvenance,
    /// Contact schema provenance.
    pub contact_schema: SchemaProvenance,
    /// The Account section.
    pub accounts: Section<Account>,
    /// The Contact section.
    pub contacts: Section<Contact>,
    /// The individual Account reads, in input order.
    pub account_reads: Vec<(RecordId, Result<Account, SfError>)>,
}

impl ExtractionReport {
    /// Whether both sections are complete and every record read succeeded.
    pub fn is_complete(&self) -> bool {
        matches!(self.accounts.completion, Completion::Complete)
            && matches!(self.contacts.completion, Completion::Complete)
            && self.account_reads.iter().all(|(_, r)| r.is_ok())
    }

    /// The resume points of the incomplete, resumable sections.
    pub fn resume(&self) -> ExtractionResume {
        ExtractionResume {
            accounts: self.accounts.completion.resume().cloned(),
            contacts: self.contacts.completion.resume().cloned(),
        }
    }
}

/// Where the selected fields came from: the describe document of the context's API version.
#[derive(Clone, Debug)]
pub struct SchemaProvenance {
    /// The object.
    pub object: SObjectName,
    /// The API version of the describe call.
    pub api_version: ApiVersion,
    /// The selected fields with their described type.
    pub selected: Vec<SelectedField>,
    /// Optional fields of the schema the org does not have (not selected).
    pub missing_optional: Vec<FieldName>,
}

/// One selected field, as described.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectedField {
    /// The selected path (`Name`, `Account.Name`).
    pub path: String,
    /// The described type (`string`, `picklist`, `reference`, ...).
    pub field_type: String,
    /// Whether it is a custom field.
    pub custom: bool,
    /// Whether it can be null.
    pub nillable: bool,
}

/// One section's records and completion.
#[derive(Debug)]
pub struct Section<T> {
    /// The statement.
    pub soql: Soql,
    /// Whether this run resumed an earlier one.
    pub resumed: bool,
    /// Records received in this run, in order.
    pub records: Vec<T>,
    /// Metadata of each page received.
    pub pages: Vec<QueryPageMeta>,
    /// How the section ended.
    pub completion: Completion,
}

/// How a section ended.
#[derive(Debug)]
pub enum Completion {
    /// Every batch was received (`done: true`).
    Complete,
    /// The page budget ended the section while batches remained.
    Bounded {
        /// Pass back to continue.
        resume: QueryResume,
    },
    /// A batch failed; its locator is kept for a later resume.
    Interrupted {
        /// Pass back to retry the failed batch and continue.
        resume: QueryResume,
        /// The failure.
        error: SfError,
    },
    /// The locator chain cannot be resumed (expired or invalid locator): restart the section
    /// from its first batch. The records of this run are a partial prefix.
    RestartRequired {
        /// The failure.
        error: SfError,
    },
}

impl Completion {
    /// The resume point, for `Bounded` and `Interrupted`.
    pub fn resume(&self) -> Option<&QueryResume> {
        match self {
            Completion::Bounded { resume } | Completion::Interrupted { resume, .. } => Some(resume),
            _ => None,
        }
    }
}

/// A failure before any section ran.
#[derive(Debug)]
#[non_exhaustive]
pub enum ExtractionError {
    /// A describe call failed.
    Describe(SfError),
    /// A required field of the selected schema is missing from the org.
    MissingRequiredField {
        /// The object.
        object: SObjectName,
        /// The field.
        field: FieldName,
    },
    /// A filter needs a field the org does not have.
    FilterFieldMissing {
        /// The field.
        field: FieldName,
    },
    /// A resume point belongs to another org, API version or statement.
    ResumeMismatch {
        /// Which section.
        section: &'static str,
    },
}

impl fmt::Display for ExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtractionError::Describe(e) => write!(f, "describe failed: {e}"),
            ExtractionError::MissingRequiredField { object, field } => {
                write!(f, "{object} has no field {field}")
            }
            ExtractionError::FilterFieldMissing { field } => {
                write!(f, "a filter needs the missing field {field}")
            }
            ExtractionError::ResumeMismatch { section } => write!(
                f,
                "the {section} resume point belongs to another org, version or statement"
            ),
        }
    }
}

impl std::error::Error for ExtractionError {}

const ACCOUNT_REQUIRED: [&str; 2] = ["Id", "Name"];
const ACCOUNT_OPTIONAL: [&str; 3] = ["Industry", "AnnualRevenue", "Customer_Tier__c"];
const CONTACT_REQUIRED: [&str; 2] = ["Id", "LastName"];
const CONTACT_OPTIONAL: [&str; 4] = ["FirstName", "Email", "AccountId", "Loyalty_Tier__c"];

fn name(text: &str) -> FieldName {
    FieldName::new(text).expect("schema field names are valid API names")
}

/// Selects the schema's fields from a describe document.
fn select(
    ctx: &OrgContext,
    describe: &SObjectDescribe,
    required: &[&str],
    optional: &[&str],
) -> Result<(Vec<FieldPath>, SchemaProvenance), ExtractionError> {
    let mut paths = Vec::new();
    let mut selected = Vec::new();
    let mut missing_optional = Vec::new();
    for (field, is_required) in required
        .iter()
        .map(|f| (f, true))
        .chain(optional.iter().map(|f| (f, false)))
    {
        let field = name(field);
        match describe.field(&field) {
            Some(described) => {
                selected.push(SelectedField {
                    path: described.name.to_string(),
                    field_type: described.field_type.clone(),
                    custom: described.custom,
                    nillable: described.nillable,
                });
                paths.push(FieldPath::field(described.name.clone()));
            }
            None if is_required => {
                return Err(ExtractionError::MissingRequiredField {
                    object: describe.name.clone(),
                    field,
                });
            }
            None => missing_optional.push(field),
        }
    }
    Ok((
        paths,
        SchemaProvenance {
            object: describe.name.clone(),
            api_version: ctx.version(),
            selected,
            missing_optional,
        },
    ))
}

fn has(schema: &SchemaProvenance, field: &str) -> bool {
    schema
        .selected
        .iter()
        .any(|s| s.path.eq_ignore_ascii_case(field))
}

/// Runs SF-A against `ctx`'s org.
///
/// # Errors
///
/// [`ExtractionError`] when the schema cannot be established or a resume point does not belong
/// to this org and statement. Failures during traversal are reported per section, not as `Err`.
pub async fn crm_extraction(
    ctx: &OrgContext,
    input: &ExtractionInput,
) -> Result<ExtractionReport, ExtractionError> {
    // 1. Schema.
    let account_describe = ctx.api().accounts().describe().send().await;
    let account_describe = match account_describe {
        Ok(d) => d,
        Err(e) => return Err(ExtractionError::Describe(SfError::from_concord(e).await)),
    };
    let contact_describe = match ctx.api().contacts().describe().send().await {
        Ok(d) => d,
        Err(e) => return Err(ExtractionError::Describe(SfError::from_concord(e).await)),
    };
    let (account_fields, account_schema) =
        select(ctx, &account_describe, &ACCOUNT_REQUIRED, &ACCOUNT_OPTIONAL)?;
    let (mut contact_fields, mut contact_schema) =
        select(ctx, &contact_describe, &CONTACT_REQUIRED, &CONTACT_OPTIONAL)?;
    // The parent's name through the AccountId relationship (dynamic in the typed Contact).
    if let Some(relationship) = contact_describe
        .field(&name("AccountId"))
        .and_then(|f| f.relationship_name.as_deref())
        && let Ok(path) = FieldPath::parse(&format!("{relationship}.Name"))
    {
        contact_schema.selected.push(SelectedField {
            path: path.to_string(),
            field_type: "string".to_owned(),
            custom: false,
            nillable: true,
        });
        contact_fields.push(path);
    }

    // 2. Statements: identifiers from the describe documents, user data as literals.
    let mut accounts = Soql::select(SObjectName::new("Account").expect("valid"), account_fields);
    if let Some(industry) = &input.industry {
        if !has(&account_schema, "Industry") {
            return Err(ExtractionError::FilterFieldMissing {
                field: name("Industry"),
            });
        }
        accounts = accounts.filter(Condition::new(
            name("Industry"),
            Op::Eq,
            Literal::text(industry.clone()),
        ));
    }
    let account_soql = accounts
        .order_by(name("Id"), Direction::Asc)
        .build()
        .expect("a non-empty, distinct selection");
    let mut contacts = Soql::select(SObjectName::new("Contact").expect("valid"), contact_fields);
    if let Some(suffix) = &input.email_suffix {
        if !has(&contact_schema, "Email") {
            return Err(ExtractionError::FilterFieldMissing {
                field: name("Email"),
            });
        }
        contacts = contacts.filter(Condition::new(
            name("Email"),
            Op::Like,
            Literal::like_suffix(suffix),
        ));
    }
    let contact_soql = contacts
        .order_by(name("Id"), Direction::Asc)
        .build()
        .expect("a non-empty, distinct selection");

    let account_from = bind_resume(
        ctx,
        &account_soql,
        input.resume.accounts.as_ref(),
        "Account",
    )?;
    let contact_from = bind_resume(
        ctx,
        &contact_soql,
        input.resume.contacts.as_ref(),
        "Contact",
    )?;

    // 3. Traversal.
    let accounts = traverse::<Account>(ctx, account_soql, input, account_from).await;
    let contacts = traverse::<Contact>(ctx, contact_soql, input, contact_from).await;

    // 4. Record reads, with the selected fields.
    let fields = FieldList::new(account_schema.selected.iter().map(|s| name(&s.path)))
        .expect("distinct selected fields");
    let mut account_reads = Vec::new();
    for id in &input.account_reads {
        let read = match ctx
            .api()
            .accounts()
            .get(id.clone(), fields.clone())
            .send()
            .await
        {
            Ok(account) => check_read(ctx, id, account),
            Err(e) => Err(SfError::from_concord(e).await),
        };
        account_reads.push((id.clone(), read));
    }

    Ok(ExtractionReport {
        account_schema,
        contact_schema,
        accounts,
        contacts,
        account_reads,
    })
}

/// A record read must be the requested record, with a URL on this org and version.
fn check_read(ctx: &OrgContext, id: &RecordId, account: Account) -> Result<Account, SfError> {
    use concord_example_salesforce_sdk::{ProtocolError, ProtocolErrorKind};
    let mismatch = || {
        SfError::from(ProtocolError::new(
            "accounts.get",
            ProtocolErrorKind::RecordLinkMismatch,
        ))
    };
    if account.id != *id {
        return Err(mismatch());
    }
    if let Some(url) = &account.attributes.url {
        let (object, linked) = ctx.record_link(url)?;
        if object.as_str() != "Account" || linked != *id {
            return Err(mismatch());
        }
    }
    Ok(account)
}

fn bind_resume(
    ctx: &OrgContext,
    soql: &Soql,
    resume: Option<&QueryResume>,
    section: &'static str,
) -> Result<Resume<QueryLocator>, ExtractionError> {
    match resume {
        None => Ok(Resume::Start),
        Some(r) => {
            if r.instance != ctx.instance().as_str()
                || r.version != ctx.version()
                || r.soql != soql.as_str()
            {
                return Err(ExtractionError::ResumeMismatch { section });
            }
            Ok(r.at.clone())
        }
    }
}

async fn traverse<R: QueryRecord>(
    ctx: &OrgContext,
    soql: Soql,
    input: &ExtractionInput,
    from: Resume<QueryLocator>,
) -> Section<R> {
    let resumed = from != Resume::Start;
    let mut pages = QueryPages::<R>::new(ctx, soql.clone());
    if let Some(batch) = input.batch_size {
        pages = pages.batch_size(batch);
    }
    let mut chain = sequential(from, input.page_budget, |key| pages.fetch(key));
    let mut records = Vec::new();
    let mut metas = Vec::new();
    let mut failure = None;
    while let Some(page) = chain.next().await {
        match page {
            Ok(page) => {
                metas.push(page.meta);
                records.extend(page.items);
            }
            Err(error) => failure = Some(error),
        }
    }
    let report = chain.finish();
    let bind = |at: Resume<QueryLocator>| QueryResume {
        instance: ctx.instance().as_str().to_owned(),
        version: ctx.version(),
        soql: soql.as_str().to_owned(),
        at,
    };
    let completion = match (report.outcome, failure) {
        (Outcome::Exhausted, None) => Completion::Complete,
        (_, Some(error)) if error.is_expired_locator() => Completion::RestartRequired { error },
        (_, Some(error)) => Completion::Interrupted {
            resume: bind(report.resume.unwrap_or(Resume::Start)),
            error,
        },
        (_, None) => Completion::Bounded {
            resume: bind(report.resume.unwrap_or(Resume::Start)),
        },
    };
    Section {
        soql,
        resumed,
        records,
        pages: metas,
        completion,
    }
}
