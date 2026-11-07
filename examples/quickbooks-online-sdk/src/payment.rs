//! The bounded Payment entity: one positive amount applied to one invoice (PROVENANCE.md, M1).

use serde::{Deserialize, Serialize, Serializer};

use crate::{Decimal, EntityId, Error, MetaData, Reference};

/// A transaction linked to a payment line.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LinkedTxn {
    /// `TxnId`: the linked entity's ID, within this connection's realm.
    pub txn_id: EntityId,
    /// `TxnType`: `Invoice` for the payment this slice creates.
    pub txn_type: String,
}

/// A payment line returned by QuickBooks. Unlike an invoice line, it has no `DetailType`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PaymentLine {
    /// `Amount`, kept as exact decimal text.
    pub amount: Decimal,
    /// `LinkedTxn`: the transactions this amount pays.
    #[serde(default)]
    pub linked_txn: Vec<LinkedTxn>,
}

/// Selected fields of the Payment entity returned after recording a payment.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Payment {
    /// `Id`, unique within the realm.
    pub id: EntityId,
    /// `SyncToken`, the object's version.
    pub sync_token: String,
    /// `CustomerRef`: the customer whose invoice is paid.
    pub customer_ref: Reference,
    /// `TotalAmt`: the amount received, kept as exact decimal text.
    pub total_amt: Decimal,
    /// `Line`: the payment's allocations.
    #[serde(default)]
    pub line: Vec<PaymentLine>,
    /// `UnappliedAmt`, when supplied by QuickBooks.
    pub unapplied_amt: Option<Decimal>,
    /// `MetaData`.
    pub meta_data: Option<MetaData>,
}

/// Create a payment response: `{"Payment": {..}, "time": ..}`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PaymentResponse {
    /// The recorded payment.
    #[serde(rename = "Payment")]
    pub payment: Payment,
    /// The server's timestamp.
    pub time: Option<String>,
}

/// A payment for exactly one invoice: `TotalAmt` equals its single line's `Amount`, and
/// `LinkedTxn.TxnType` is always `Invoice`. IDs are resolved only within the client's realm.
/// QuickBooks validates invoice ownership, currency and outstanding balance. This slice does
/// not support multicurrency, credits, refunds or unapplied payments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewPayment {
    customer: EntityId,
    invoice: EntityId,
    amount: Decimal,
}

impl NewPayment {
    /// Record `amount` received for `customer` against `invoice`. Partial payments are allowed;
    /// the amount is sent verbatim, without rounding or conversion through floating point.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidInput`] for zero or a negative amount. This is an SDK restriction to
    /// payments of received funds; QuickBooks supports other payment arrangements outside it.
    pub fn new(customer: EntityId, invoice: EntityId, amount: Decimal) -> Result<Self, Error> {
        if amount.as_str().starts_with('-')
            || !amount.as_str().bytes().any(|b| matches!(b, b'1'..=b'9'))
        {
            return Err(Error::InvalidInput {
                what: "positive payment amount",
            });
        }
        Ok(Self {
            customer,
            invoice,
            amount,
        })
    }
}

impl Serialize for NewPayment {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct CustomerRef<'a> {
            value: &'a EntityId,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Transaction<'a> {
            txn_id: &'a EntityId,
            txn_type: &'static str,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Allocation<'a> {
            amount: &'a Decimal,
            linked_txn: [Transaction<'a>; 1],
        }
        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Wire<'a> {
            customer_ref: CustomerRef<'a>,
            total_amt: &'a Decimal,
            line: [Allocation<'a>; 1],
        }
        Wire {
            customer_ref: CustomerRef {
                value: &self.customer,
            },
            total_amt: &self.amount,
            line: [Allocation {
                amount: &self.amount,
                linked_txn: [Transaction {
                    txn_id: &self.invoice,
                    txn_type: "Invoice",
                }],
            }],
        }
        .serialize(serializer)
    }
}
