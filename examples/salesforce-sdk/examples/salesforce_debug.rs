//! Diagnostic formatting and request preparation, without network I/O.
use concord_example_salesforce_sdk::OrgContext;
use concord_example_salesforce_sdk::ids::{AccessToken, ApiVersion, InstanceUrl, SObjectName};
use concord_example_salesforce_sdk::model::Account;
use concord_example_salesforce_sdk::soql::{Condition, FieldPath, Literal, Op, Soql};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let ctx = OrgContext::new(
        concord::reqwest::Client::new(),
        InstanceUrl::parse("https://org.example.test")?,
        ApiVersion::PINNED,
        AccessToken::new("synthetic-access-token")?,
    )?;
    let query = Soql::select(
        SObjectName::new("Account")?,
        [FieldPath::parse("Id")?, FieldPath::parse("Name")?],
    )
    .filter(Condition::new(
        FieldPath::parse("Name")?,
        Op::Eq,
        Literal::text("Private customer"),
    ))
    .build()?;
    let prepared = ctx.api().query().accounts(query).prepare()?;
    println!("{ctx:?} {prepared:?}");

    let account: Account = serde_json::from_str(
        r#"{"attributes":{"type":"Account"},"Id":"001A00000000001AAA",
            "Name":"Private customer","Description":"Private record content"}"#,
    )?;
    // Ordinary Debug still shows the record id; the annotated contents are redacted.
    println!("{account:?}");
    println!("{account:#?}");
    Ok(())
}
