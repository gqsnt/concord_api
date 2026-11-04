//! The frozen initial consumer: an application that mirrors CRM contacts into Stripe customers
//! on a connected account.
//!
//! Written as a downstream application would write it, from the SDK's rustdoc and `PROVIDER.md`
//! only: request bodies are filled through their public fields (`..Default::default()` for the
//! rest), operations come from [`StripeContext::api`], and every call goes through the SDK's
//! [`send`] so the `Request-Id` is kept. Frozen at stage 1 of the exercise: do not edit.

use concord_example_stripe_sdk::models::Address;
use concord_example_stripe_sdk::params::{CustomerCreate, CustomerUpdate};
use concord_example_stripe_sdk::{
    Customer, CustomerId, Error, IdempotencyKey, Limit, Metadata, MetadataPatch, Patch,
    StripeContext, send,
};

/// The metadata key under which the CRM id is stored on the Stripe customer.
pub const CRM_ID_KEY: &str = "crm_id";

/// A contact as the CRM knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrmContact {
    /// The CRM's own id, kept in the customer's metadata.
    pub crm_id: String,
    /// Display name.
    pub name: String,
    /// Email address.
    pub email: String,
    /// Two-letter country code.
    pub country: String,
    /// City, if known.
    pub city: Option<String>,
    /// Preferred locale, if known (`fr-FR`).
    pub locale: Option<String>,
}

/// A correction of an already mirrored contact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Correction {
    /// The corrected display name.
    pub name: String,
    /// Remove the phone number Stripe holds.
    pub drop_phone: bool,
    /// Metadata keys of an earlier integration to remove.
    pub retire_keys: Vec<String>,
}

/// What the application keeps about a Stripe customer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactProfile {
    /// The Stripe customer.
    pub customer: CustomerId,
    /// Display name.
    pub name: Option<String>,
    /// Email.
    pub email: Option<String>,
    /// Country of the billing address.
    pub country: Option<String>,
    /// The CRM id from the metadata.
    pub crm_id: Option<String>,
}

impl ContactProfile {
    fn of(customer: &Customer) -> Self {
        Self {
            customer: customer.id.clone(),
            name: customer.name.clone(),
            email: customer.email.clone(),
            country: customer.address.as_ref().and_then(|a| a.country.clone()),
            crm_id: customer.metadata.get(CRM_ID_KEY).cloned(),
        }
    }
}

/// The outcome of one write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Written {
    /// The customer as Stripe returned it.
    pub profile: ContactProfile,
    /// Stripe's `Request-Id`.
    pub request_id: Option<String>,
    /// Whether Stripe replayed an earlier result for the same idempotency key.
    pub replayed: bool,
}

/// `POST /v1/customers`: creates the Stripe customer of `contact` with the application's
/// idempotency key.
///
/// # Errors
///
/// The SDK's [`Error`]: a refused value, a transport failure or Stripe's error document.
pub async fn mirror_contact(
    stripe: &StripeContext,
    contact: &CrmContact,
    key: IdempotencyKey,
) -> Result<Written, Error> {
    let params = CustomerCreate {
        name: Some(contact.name.clone()),
        email: Some(contact.email.clone()),
        address: Some(Address {
            country: Some(contact.country.clone()),
            city: contact.city.clone(),
            ..Address::default()
        }),
        preferred_locales: contact.locale.iter().cloned().collect(),
        metadata: Metadata::from([(CRM_ID_KEY.to_owned(), contact.crm_id.clone())]),
        ..CustomerCreate::default()
    };
    let operation = stripe
        .api()
        .customers()
        .create(params)
        .set_idempotency_key(key);
    let response = send(operation.prepare()?).await?;
    Ok(Written {
        profile: ContactProfile::of(&response.value),
        request_id: response.meta.request_id,
        replayed: response.meta.idempotent_replayed,
    })
}

/// `POST /v1/customers/{id}`: applies `correction` (fields not mentioned stay unchanged).
///
/// # Errors
///
/// The SDK's [`Error`].
pub async fn correct_contact(
    stripe: &StripeContext,
    customer: &CustomerId,
    correction: &Correction,
    key: IdempotencyKey,
) -> Result<Written, Error> {
    let mut metadata = MetadataPatch::new();
    for retired in &correction.retire_keys {
        metadata = metadata.unset(retired.clone());
    }
    let params = CustomerUpdate {
        name: Some(Patch::Set(correction.name.clone())),
        phone: correction.drop_phone.then_some(Patch::Unset),
        metadata: (!correction.retire_keys.is_empty()).then_some(metadata),
        ..CustomerUpdate::default()
    };
    let operation = stripe
        .api()
        .customers()
        .update(customer.clone(), params)
        .set_idempotency_key(key);
    let response = send(operation.prepare()?).await?;
    Ok(Written {
        profile: ContactProfile::of(&response.value),
        request_id: response.meta.request_id,
        replayed: response.meta.idempotent_replayed,
    })
}

/// `GET /v1/customers/{id}`.
///
/// # Errors
///
/// The SDK's [`Error`].
pub async fn fetch_profile(
    stripe: &StripeContext,
    customer: &CustomerId,
) -> Result<ContactProfile, Error> {
    let operation = stripe.api().customers().retrieve(customer.clone());
    let response = send(operation.prepare()?).await?;
    Ok(ContactProfile::of(&response.value))
}

/// `GET /v1/customers?email=..&limit=..`: the first page of customers with exactly this email
/// (the application expects at most a handful; it does not page further).
///
/// # Errors
///
/// The SDK's [`Error`].
pub async fn find_by_email(
    stripe: &StripeContext,
    email: &str,
    limit: Limit,
) -> Result<Vec<ContactProfile>, Error> {
    let operation = stripe
        .api()
        .customers()
        .list()
        .set_email(email)
        .set_limit(limit);
    let response = send(operation.prepare()?).await?;
    Ok(response.value.data.iter().map(ContactProfile::of).collect())
}
