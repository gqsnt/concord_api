// The data types that the page names and does not print, cut to the fields that its code uses.

macro_rules! ids {
    ($($name:ident),*) => { $(
        #[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);
        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                $name(id.to_owned())
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    )* };
}
ids!(AccountId, CustomerId, PaymentIntentId, RefundId, SubscriptionId, InvoiceId,
    BalanceTransactionId, EventId, ChargeId, PriceId, PayoutId);

macro_rules! objects {
    ($($name:ident: $id:ty),*) => { $(
        #[derive(Debug, serde::Deserialize)]
        pub struct $name {
            pub id: $id,
        }
        impl Object for $name {
            fn id(&self) -> &str {
                &self.id.0
            }
        }
    )* };
}
objects!(Refund: RefundId, Subscription: SubscriptionId, BalanceTransaction: BalanceTransactionId,
    Event: EventId);

pub type Metadata = std::collections::BTreeMap<String, String>;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Currency(pub String);
#[derive(Clone, Debug, serde::Serialize)]
pub struct NewItem {
    pub price: PriceId,
    pub quantity: Option<u32>,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionFilter {
    Active,
    All,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Open,
    Paid,
}
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectionMethod {
    ChargeAutomatically,
    SendInvoice,
}
#[derive(Clone, Debug)]
pub enum FilePurpose {
    DisputeEvidence,
    IdentityDocument,
}
impl FilePurpose {
    pub fn as_str(&self) -> &'static str {
        match self {
            FilePurpose::DisputeEvidence => "dispute_evidence",
            FilePurpose::IdentityDocument => "identity_document",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Upload {
    pub name: String,
    pub content_type: String,
    pub bytes: concord::bytes::Bytes,
}
#[derive(Debug, serde::Deserialize)]
pub struct Customer {
    pub id: CustomerId,
    pub email: Option<String>,
}
impl Object for Customer {
    fn id(&self) -> &str {
        &self.id.0
    }
}
#[derive(Debug, serde::Deserialize)]
pub struct PaymentIntent {
    pub id: PaymentIntentId,
    pub status: String,
    pub customer: Option<Expandable<Customer>>,
}
impl Object for PaymentIntent {
    fn id(&self) -> &str {
        &self.id.0
    }
}
#[derive(Debug, serde::Deserialize)]
pub struct Invoice {
    pub id: InvoiceId,
    pub amount_due: i64,
    pub currency: String,
}
impl Object for Invoice {
    fn id(&self) -> &str {
        &self.id.0
    }
}
#[derive(Debug, serde::Deserialize)]
pub struct Balance {}
#[derive(Debug, serde::Deserialize)]
pub struct StripeFile {
    pub id: String,
}
#[derive(Debug, serde::Deserialize)]
pub struct Deleted<I> {
    pub id: I,
    pub deleted: bool,
}
#[derive(Debug, serde::Deserialize)]
pub struct StripeProblem {
    pub error: StripeError,
}
#[derive(Debug, serde::Deserialize)]
pub struct Declined {
    pub error: StripeError,
}
#[derive(Debug, serde::Deserialize)]
pub struct StripeError {
    pub code: Option<String>,
    pub decline_code: Option<String>,
    pub message: Option<String>,
}
