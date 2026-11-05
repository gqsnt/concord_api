//! The updated consumer (stage 3): the CRM-mirroring application of [`crate::before`] now also
//! records the customer's default payment method for invoices and shows its card summary.
//!
//! It uses the capability added at stage 2 through public exports only: the nested
//! `invoice_settings[default_payment_method]` body field (`InvoiceSettingsParams`) and the
//! `invoice_settings.default_payment_method` expansion decoded as `Expandable<PaymentMethod>`.
//! The frozen consumer stays as it was beside it.

use concord_example_stripe_sdk::models::Address;
use concord_example_stripe_sdk::params::{CustomerCreate, CustomerUpdate, InvoiceSettingsParams};
use concord_example_stripe_sdk::{
    Customer, CustomerId, Error, ExpandPath, Expandable, IdempotencyKey, Limit, Metadata,
    PaymentMethod, PaymentMethodId, StripeContext, send,
};

use crate::before::{CRM_ID_KEY, CrmContact};

/// The expansion on a single customer (retrieve, create, update).
pub const DEFAULT_PAYMENT_METHOD: &str = "invoice_settings.default_payment_method";
/// The same expansion on a customer list (Stripe: list expansions start with `data`).
pub const LIST_DEFAULT_PAYMENT_METHOD: &str = "data.invoice_settings.default_payment_method";

/// The customer's default payment method for invoices, as far as the application shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DefaultPayment {
    /// None is set.
    Unset,
    /// Only the id is known: the response did not carry the expanded object.
    IdOnly(PaymentMethodId),
    /// A card.
    Card {
        /// The payment method.
        id: PaymentMethodId,
        /// Card brand, as Stripe sends it.
        brand: String,
        /// Last four digits.
        last4: String,
        /// `MM/YYYY`.
        expires: String,
    },
    /// Another payment method type (`sepa_debit`, `us_bank_account`, ...).
    Other {
        /// The payment method.
        id: PaymentMethodId,
        /// Its type.
        kind: String,
    },
}

impl DefaultPayment {
    fn of(customer: &Customer) -> Self {
        match customer.default_payment_method() {
            None => Self::Unset,
            Some(Expandable::Id(id)) => Self::IdOnly(id.clone()),
            Some(Expandable::Object(method)) => Self::of_method(method),
        }
    }

    fn of_method(method: &PaymentMethod) -> Self {
        match &method.card {
            Some(card) => Self::Card {
                id: method.id.clone(),
                brand: card.brand.clone(),
                last4: card.last4.clone(),
                expires: format!("{:02}/{}", card.exp_month, card.exp_year),
            },
            None => Self::Other {
                id: method.id.clone(),
                kind: method.kind.clone(),
            },
        }
    }
}

/// What the billing screen shows about a customer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BillingProfile {
    /// The Stripe customer.
    pub customer: CustomerId,
    /// The CRM id from the metadata.
    pub crm_id: Option<String>,
    /// The default payment method for invoices.
    pub default_payment: DefaultPayment,
}

impl BillingProfile {
    fn of(customer: &Customer) -> Self {
        Self {
            customer: customer.id.clone(),
            crm_id: customer.metadata.get(CRM_ID_KEY).cloned(),
            default_payment: DefaultPayment::of(customer),
        }
    }
}

fn expansion(path: &str) -> Vec<ExpandPath> {
    vec![ExpandPath::new(path).expect("a static expansion path is valid")]
}

/// `POST /v1/customers` like [`crate::before::mirror_contact`], optionally with a default
/// payment method already attached elsewhere (`invoice_settings[default_payment_method]`).
/// With `None`, the request is byte-for-byte the frozen consumer's.
///
/// # Errors
///
/// The SDK's [`Error`].
pub async fn mirror_contact_with_default(
    stripe: &StripeContext,
    contact: &CrmContact,
    default_payment_method: Option<PaymentMethodId>,
    key: IdempotencyKey,
) -> Result<BillingProfile, Error> {
    let params = CustomerCreate {
        name: Some(contact.name.clone()),
        email: Some(contact.email.clone()),
        address: Some(Address {
            country: Some(contact.country.clone()),
            city: contact.city.clone(),
            ..Address::default()
        }),
        preferred_locales: contact.locale.iter().cloned().collect(),
        invoice_settings: default_payment_method.map(|id| InvoiceSettingsParams {
            default_payment_method: Some(id),
        }),
        metadata: Metadata::from([(CRM_ID_KEY.to_owned(), contact.crm_id.clone())]),
        ..CustomerCreate::default()
    };
    let operation = stripe
        .api()
        .customers()
        .create(params)
        .set_idempotency_key(key);
    let response = send(operation.prepare()?).await?;
    Ok(BillingProfile::of(&response.value))
}

/// `POST /v1/customers/{id}` with `invoice_settings[default_payment_method]`, asking Stripe to
/// expand the new default in the response (`expand[]` in the form body) so the card summary
/// comes back in the same round trip.
///
/// # Errors
///
/// The SDK's [`Error`] (Stripe refuses a payment method not attached to the customer).
pub async fn set_default_payment_method(
    stripe: &StripeContext,
    customer: &CustomerId,
    method: PaymentMethodId,
    key: IdempotencyKey,
) -> Result<BillingProfile, Error> {
    let params = CustomerUpdate {
        invoice_settings: Some(InvoiceSettingsParams {
            default_payment_method: Some(method),
        }),
        expand: expansion(DEFAULT_PAYMENT_METHOD),
        ..CustomerUpdate::default()
    };
    let operation = stripe
        .api()
        .customers()
        .update(customer.clone(), params)
        .set_idempotency_key(key);
    let response = send(operation.prepare()?).await?;
    Ok(BillingProfile::of(&response.value))
}

/// `GET /v1/customers/{id}?expand[]=invoice_settings.default_payment_method`.
///
/// # Errors
///
/// The SDK's [`Error`].
pub async fn billing_profile(
    stripe: &StripeContext,
    customer: &CustomerId,
) -> Result<BillingProfile, Error> {
    let operation = stripe
        .api()
        .customers()
        .retrieve(customer.clone())
        .set_expand(expansion(DEFAULT_PAYMENT_METHOD));
    let response = send(operation.prepare()?).await?;
    Ok(BillingProfile::of(&response.value))
}

/// `GET /v1/customers?email=..&limit=..&expand[]=data.invoice_settings.default_payment_method`:
/// the first page, like [`crate::before::find_by_email`], with each default payment method.
///
/// # Errors
///
/// The SDK's [`Error`].
pub async fn billing_profiles_by_email(
    stripe: &StripeContext,
    email: &str,
    limit: Limit,
) -> Result<Vec<BillingProfile>, Error> {
    let operation = stripe
        .api()
        .customers()
        .list()
        .set_email(email)
        .set_limit(limit)
        .set_expand(expansion(LIST_DEFAULT_PAYMENT_METHOD));
    let response = send(operation.prepare()?).await?;
    Ok(response.value.data.iter().map(BillingProfile::of).collect())
}
