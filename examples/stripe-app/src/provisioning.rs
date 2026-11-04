//! ST-B — Stripe form and idempotency scenario: provision a small catalog, a customer and a
//! subscription on a connected account, with an application-supplied idempotency policy, and
//! return a trace of exactly which writes were attempted and which Stripe recognized.
//!
//! # The flow
//!
//! | Step | Operation | Form features exercised |
//! |---|---|---|
//! | [`Step::CreateProduct`] | `POST /v1/products` | nested `marketing_features[0][name]`, `metadata[..]`, repeated `images[]` |
//! | [`Step::CreatePrice`] | `POST /v1/prices` | `product=` from step 1, nested `recurring[interval]` |
//! | [`Step::CreateCustomer`] | `POST /v1/customers` | nested `address[..]`, repeated `preferred_locales[]` |
//! | [`Step::UpdateCustomer`] | `POST /v1/customers/{id}` | omitted fields vs empty values (`description=`, `metadata[key]=`) |
//! | [`Step::CreateSubscription`] | `POST /v1/subscriptions` | indexed `items[0][price]`, repeated `expand[]` |
//! | [`Step::LinkPlatformCustomer`] | `POST /v1/customers/{id}` on the **platform** | per-request account override to none |
//!
//! Steps 1–5 act on [`ProvisioningPlan::target`] through a per-request `Stripe-Account` override
//! ([`AccountOverride`]); step 6, when requested, explicitly acts on the platform's own account
//! even if the context names a connected account. Each step needs the ids of the previous ones,
//! so the flow is sequential and stops at the first step that does not end recognized.
//!
//! # Idempotency policy (the application's, not the SDK's)
//!
//! - Every write carries the key the [`IdempotencyPolicy`] assigns to its step. A step without a
//!   key is not sent ([`Prevented::MissingKey`]). Neither the SDK nor `concord-pagination`
//!   generates keys or repeats writes.
//! - A **lost response** (no response received: the request may or may not have been executed)
//!   is repeated **unchanged** — the same prepared bytes, the same key — up to
//!   [`IdempotencyPolicy::repeats_after_lost_response`] times. Stripe then returns the saved result
//!   (`Idempotent-Replayed: true`) if the first request was executed. Any response (a 2xx, a 4xx,
//!   a 5xx) ends the step: a `500` is saved by Stripe under the key and is not repeated here,
//!   and nothing is repeated with a new key.
//! - The application's [`KeyLedger`] remembers which request each key was used for (method,
//!   path, query, `Stripe-Account` and body bytes). Reusing a key for a **different** request is
//!   refused before sending ([`Prevented::KeyReusedWithDifferentRequest`]). A ledger is the
//!   application's storage: when it is not shared (another process, a lost ledger), Stripe's own
//!   check refuses the changed request (`type: idempotency_error`), recorded as
//!   [`Attempt::Rejected`].
//!
//! No step promises exactly-once execution: an unresolved step ([`StepResult::Unresolved`])
//! means the outcome is unknown and must be reconciled (retrieve the object, the dashboard,
//! webhooks), as Stripe's documentation advises.
//!
//! # Diagnostics
//!
//! `Debug` of the trace shows steps, attempts, object ids and request ids; idempotency keys and
//! account ids are redacted by their own types, and errors follow the SDK's policy.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use concord::Prepared;
use concord_example_stripe_sdk::params::{
    CustomerCreate, CustomerUpdate, PriceCreate, PriceProduct, ProductCreate, RecurringParams,
    SubscriptionCreate, SubscriptionItemCreate,
};
use concord_example_stripe_sdk::send::ApiErrorType;
use concord_example_stripe_sdk::{
    AccountOverride, Currency, CustomerId, Error, ExpandPath, IdempotencyKey, InvoiceId, Metadata,
    MetadataPatch, Object, OnAccount, PriceId, ProductId, StripeContext, SubscriptionId, TaxRateId,
    ValueError, send,
};

/// One write of the flow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Step {
    /// `POST /v1/products`.
    CreateProduct,
    /// `POST /v1/prices`.
    CreatePrice,
    /// `POST /v1/customers`.
    CreateCustomer,
    /// `POST /v1/customers/{id}`.
    UpdateCustomer,
    /// `POST /v1/subscriptions`.
    CreateSubscription,
    /// `POST /v1/customers/{id}` on the platform account.
    LinkPlatformCustomer,
}

impl Step {
    /// Every step, in flow order.
    pub const ALL: [Step; 6] = [
        Step::CreateProduct,
        Step::CreatePrice,
        Step::CreateCustomer,
        Step::UpdateCustomer,
        Step::CreateSubscription,
        Step::LinkPlatformCustomer,
    ];

    /// A short stable name, such as `create-product`.
    pub fn name(self) -> &'static str {
        match self {
            Step::CreateProduct => "create-product",
            Step::CreatePrice => "create-price",
            Step::CreateCustomer => "create-customer",
            Step::UpdateCustomer => "update-customer",
            Step::CreateSubscription => "create-subscription",
            Step::LinkPlatformCustomer => "link-platform-customer",
        }
    }

    fn operation(self) -> &'static str {
        match self {
            Step::CreateProduct => "POST /v1/products",
            Step::CreatePrice => "POST /v1/prices",
            Step::CreateCustomer => "POST /v1/customers",
            Step::UpdateCustomer | Step::LinkPlatformCustomer => "POST /v1/customers/{id}",
            Step::CreateSubscription => "POST /v1/subscriptions",
        }
    }
}

/// The price of the plan; its product is the one created by [`Step::CreateProduct`].
#[derive(Clone, Debug, PartialEq)]
pub struct PricePlan {
    /// Currency.
    pub currency: Currency,
    /// Unit amount in the smallest currency unit.
    pub unit_amount: i64,
    /// Recurring part (absent: one-time).
    pub recurring: Option<RecurringParams>,
    /// Lookup key.
    pub lookup_key: Option<String>,
    /// Metadata.
    pub metadata: Metadata,
}

/// The subscription of the plan: the created customer, one item of the created price.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SubscriptionPlan {
    /// Item quantity.
    pub quantity: Option<u64>,
    /// Subscription metadata.
    pub metadata: Metadata,
    /// Default tax rates (repeated field).
    pub default_tax_rates: Vec<TaxRateId>,
    /// Response expansions, such as `latest_invoice` (repeated field).
    pub expand: Vec<ExpandPath>,
}

/// A platform-side customer record to link to the provisioned customer: written on the
/// platform's own account (no `Stripe-Account`), with `metadata[key] = <connected customer id>`.
#[derive(Clone, Debug, PartialEq)]
pub struct PlatformLink {
    /// The platform customer.
    pub customer: CustomerId,
    /// The metadata key that receives the connected customer id.
    pub metadata_key: String,
}

/// What to provision.
#[derive(Clone, Debug, PartialEq)]
pub struct ProvisioningPlan {
    /// The account steps 1–5 act on (a per-request `Stripe-Account` override).
    pub target: AccountOverride,
    /// The product.
    pub product: ProductCreate,
    /// The price.
    pub price: PricePlan,
    /// The customer.
    pub customer: CustomerCreate,
    /// A correction applied right after creating the customer (fields left `None` are omitted).
    pub customer_update: CustomerUpdate,
    /// The subscription.
    pub subscription: SubscriptionPlan,
    /// The optional platform-side link.
    pub platform_link: Option<PlatformLink>,
}

/// The application's idempotency policy: one key per step, and how often an unchanged request is
/// repeated after a lost response.
#[derive(Clone, PartialEq, Eq)]
pub struct IdempotencyPolicy {
    keys: BTreeMap<Step, IdempotencyKey>,
    /// Repeats of the unchanged request (same bytes, same key) after a response was lost.
    pub repeats_after_lost_response: u32,
}

impl IdempotencyPolicy {
    /// A policy with explicit keys.
    pub fn new(keys: BTreeMap<Step, IdempotencyKey>, repeats_after_lost_response: u32) -> Self {
        Self {
            keys,
            repeats_after_lost_response,
        }
    }

    /// One key per step derived from an application operation id: `<operation>/<step name>`
    /// (for example an order or onboarding id the application already stores).
    ///
    /// # Errors
    ///
    /// [`ValueError::IdempotencyKey`] when a derived key is not a valid key.
    pub fn derived(operation: &str, repeats_after_lost_response: u32) -> Result<Self, ValueError> {
        let mut keys = BTreeMap::new();
        for step in Step::ALL {
            keys.insert(
                step,
                IdempotencyKey::new(format!("{operation}/{}", step.name()))?,
            );
        }
        Ok(Self::new(keys, repeats_after_lost_response))
    }

    /// The key of `step`.
    pub fn key(&self, step: Step) -> Option<&IdempotencyKey> {
        self.keys.get(&step)
    }
}

impl fmt::Debug for IdempotencyPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IdempotencyPolicy")
            .field("steps_with_keys", &self.keys.keys().collect::<Vec<_>>())
            .field(
                "repeats_after_lost_response",
                &self.repeats_after_lost_response,
            )
            .finish()
    }
}

/// The application's record of which request each idempotency key was used for. `Debug` shows
/// the number of keys only.
#[derive(Clone, Default)]
pub struct KeyLedger {
    entries: HashMap<IdempotencyKey, Vec<u8>>,
}

impl KeyLedger {
    /// An empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Keys recorded.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no key is recorded.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Records `key` for `request`; `false` (nothing recorded) when the key was already used for
    /// a different request.
    fn admit(&mut self, key: &IdempotencyKey, request: Vec<u8>) -> bool {
        match self.entries.get(key) {
            Some(previous) => *previous == request,
            None => {
                self.entries.insert(key.clone(), request);
                true
            }
        }
    }
}

impl fmt::Debug for KeyLedger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyLedger")
            .field("keys", &self.entries.len())
            .finish()
    }
}

/// One attempt of a write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attempt {
    /// Stripe answered with a 2xx: the write is recognized.
    Accepted {
        /// `Request-Id`.
        request_id: Option<String>,
        /// `Idempotent-Replayed: true`: the saved result of an earlier request with this key.
        replayed: bool,
    },
    /// No response arrived: the request may or may not have been executed.
    ResponseLost,
    /// Stripe answered with a non-2xx status.
    Rejected {
        /// The status.
        status: u16,
        /// Stripe's error type, if the document was readable.
        error_type: Option<ApiErrorType>,
        /// Stripe's error code.
        code: Option<String>,
        /// `Request-Id`.
        request_id: Option<String>,
    },
    /// Another failure (the request could not be built, a 2xx body did not decode).
    Failed,
}

/// Why a write was not sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Prevented {
    /// The policy has no key for this step.
    MissingKey,
    /// The step's key was already used for a different request (method, path, query, account
    /// or body).
    KeyReusedWithDifferentRequest,
}

/// How a step ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepResult {
    /// Stripe recognized the write: the object id, and whether the response was a replay.
    Recognized {
        /// The created or updated object's id.
        id: String,
        /// The recognizing response was a replay of a saved result.
        replayed: bool,
    },
    /// Nothing was sent.
    Prevented(Prevented),
    /// Stripe refused it (see the last attempt).
    Rejected,
    /// Every attempt lost its response: the outcome is unknown.
    Unresolved,
    /// Another failure (see the last attempt and [`ProvisioningTrace::error`]).
    Failed,
}

/// One write in the trace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteRecord {
    /// The step.
    pub step: Step,
    /// The operation, such as `POST /v1/products`.
    pub operation: &'static str,
    /// The requested account.
    pub account: AccountOverride,
    /// The key used (redacted in `Debug`).
    pub key: Option<IdempotencyKey>,
    /// Every attempt, in order; empty when nothing was sent.
    pub attempts: Vec<Attempt>,
    /// How the step ended.
    pub result: StepResult,
}

/// The ids of what was provisioned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Provisioned {
    /// The product.
    pub product: Option<ProductId>,
    /// The price.
    pub price: Option<PriceId>,
    /// The customer.
    pub customer: Option<CustomerId>,
    /// The subscription.
    pub subscription: Option<SubscriptionId>,
    /// The subscription's latest invoice.
    pub latest_invoice: Option<InvoiceId>,
}

/// The outcome of [`provision`].
#[derive(Debug)]
pub struct ProvisioningTrace {
    /// Every write considered, in order.
    pub writes: Vec<WriteRecord>,
    /// What exists now, as far as Stripe recognized it.
    pub provisioned: Provisioned,
    /// The step the flow stopped at, if it did not complete.
    pub stopped_at: Option<Step>,
    /// The SDK error of the last failed attempt, unchanged.
    pub error: Option<Error>,
}

impl ProvisioningTrace {
    /// Whether every planned step was recognized.
    pub fn is_complete(&self) -> bool {
        self.stopped_at.is_none()
    }

    /// The record of `step`, if it was considered.
    pub fn write(&self, step: Step) -> Option<&WriteRecord> {
        self.writes.iter().find(|w| w.step == step)
    }

    /// Writes actually sent (at least one attempt), in order.
    pub fn attempted(&self) -> impl Iterator<Item = &WriteRecord> {
        self.writes.iter().filter(|w| !w.attempts.is_empty())
    }
}

/// Runs the ST-B flow. See the [module documentation](self).
///
/// The trace is the result: every outcome, including a stop, is described there (no `Err`).
pub async fn provision(
    stripe: &StripeContext,
    plan: &ProvisioningPlan,
    policy: &IdempotencyPolicy,
    ledger: &mut KeyLedger,
) -> ProvisioningTrace {
    let mut run = Run {
        policy,
        ledger,
        writes: Vec::new(),
        error: None,
    };
    let mut provisioned = Provisioned::default();
    let stopped_at = flow(stripe, plan, &mut run, &mut provisioned).await.err();
    ProvisioningTrace {
        writes: run.writes,
        provisioned,
        stopped_at,
        error: run.error,
    }
}

async fn flow(
    stripe: &StripeContext,
    plan: &ProvisioningPlan,
    run: &mut Run<'_>,
    provisioned: &mut Provisioned,
) -> Result<(), Step> {
    let api = stripe.api();
    let target = &plan.target;

    let step = Step::CreateProduct;
    let operation = api
        .products()
        .create(plan.product.clone())
        .on_account(target);
    let product = run
        .write(step, target, |key| {
            operation.clone().set_idempotency_key(key).prepare()
        })
        .await
        .ok_or(step)?;
    provisioned.product = Some(product.id.clone());

    let step = Step::CreatePrice;
    let mut price = PriceCreate::new(
        plan.price.currency.clone(),
        PriceProduct::Existing(product.id.clone()),
        plan.price.unit_amount,
    );
    price.recurring = plan.price.recurring.clone();
    price.lookup_key = plan.price.lookup_key.clone();
    price.metadata = plan.price.metadata.clone();
    let operation = api.prices().create(price).on_account(target);
    let price = run
        .write(step, target, |key| {
            operation.clone().set_idempotency_key(key).prepare()
        })
        .await
        .ok_or(step)?;
    provisioned.price = Some(price.id.clone());

    let step = Step::CreateCustomer;
    let operation = api
        .customers()
        .create(plan.customer.clone())
        .on_account(target);
    let customer = run
        .write(step, target, |key| {
            operation.clone().set_idempotency_key(key).prepare()
        })
        .await
        .ok_or(step)?;
    provisioned.customer = Some(customer.id.clone());

    let step = Step::UpdateCustomer;
    let operation = api
        .customers()
        .update(customer.id.clone(), plan.customer_update.clone())
        .on_account(target);
    run.write(step, target, |key| {
        operation.clone().set_idempotency_key(key).prepare()
    })
    .await
    .ok_or(step)?;

    let step = Step::CreateSubscription;
    let mut item = SubscriptionItemCreate::new(price.id.clone());
    item.quantity = plan.subscription.quantity;
    let mut subscription = SubscriptionCreate::new(customer.id.clone(), vec![item]);
    subscription.metadata = plan.subscription.metadata.clone();
    subscription.default_tax_rates = plan.subscription.default_tax_rates.clone();
    subscription.expand = plan.subscription.expand.clone();
    let operation = api.subscriptions().create(subscription).on_account(target);
    let subscription = run
        .write(step, target, |key| {
            operation.clone().set_idempotency_key(key).prepare()
        })
        .await
        .ok_or(step)?;
    provisioned.subscription = Some(subscription.id.clone());
    provisioned.latest_invoice = subscription
        .latest_invoice
        .as_ref()
        .map(|invoice| invoice.id().clone());

    if let Some(link) = &plan.platform_link {
        let step = Step::LinkPlatformCustomer;
        let platform = AccountOverride::Platform;
        let update = CustomerUpdate {
            metadata: Some(
                MetadataPatch::new().set(link.metadata_key.clone(), customer.id.to_string()),
            ),
            ..CustomerUpdate::default()
        };
        let operation = api
            .customers()
            .update(link.customer.clone(), update)
            .on_account(&platform);
        run.write(step, &platform, |key| {
            operation.clone().set_idempotency_key(key).prepare()
        })
        .await
        .ok_or(step)?;
    }
    Ok(())
}

struct Run<'a> {
    policy: &'a IdempotencyPolicy,
    ledger: &'a mut KeyLedger,
    writes: Vec<WriteRecord>,
    error: Option<Error>,
}

impl Run<'_> {
    /// One write: key, ledger check, then one send plus the policy's repeats after a lost
    /// response, each repeat rebuilt from the same operation and checked to be byte-identical.
    async fn write<T: Object>(
        &mut self,
        step: Step,
        account: &AccountOverride,
        build: impl Fn(IdempotencyKey) -> Result<Prepared<T>, concord::Error>,
    ) -> Option<T> {
        let mut record = WriteRecord {
            step,
            operation: step.operation(),
            account: account.clone(),
            key: self.policy.key(step).cloned(),
            attempts: Vec::new(),
            result: StepResult::Failed,
        };
        let Some(key) = record.key.clone() else {
            record.result = StepResult::Prevented(Prevented::MissingKey);
            self.writes.push(record);
            return None;
        };
        let mut fingerprint = None;
        let mut value = None;
        let mut last_error = None;
        for _ in 0..=self.policy.repeats_after_lost_response {
            let prepared = match build(key.clone()) {
                Ok(prepared) => prepared,
                Err(error) => {
                    record.attempts.push(Attempt::Failed);
                    record.result = StepResult::Failed;
                    last_error = Some(error.into());
                    break;
                }
            };
            let request = request_identity(&prepared);
            match &fingerprint {
                None => {
                    if !self.ledger.admit(&key, request.clone()) {
                        record.result =
                            StepResult::Prevented(Prevented::KeyReusedWithDifferentRequest);
                        break;
                    }
                    fingerprint = Some(request);
                }
                // A repeat must be the unchanged request.
                Some(first) if *first != request => {
                    record.result = StepResult::Prevented(Prevented::KeyReusedWithDifferentRequest);
                    break;
                }
                Some(_) => {}
            }
            match send(prepared).await {
                Ok(response) => {
                    record.attempts.push(Attempt::Accepted {
                        request_id: response.meta.request_id.clone(),
                        replayed: response.meta.idempotent_replayed,
                    });
                    record.result = StepResult::Recognized {
                        id: response.value.id().to_string(),
                        replayed: response.meta.idempotent_replayed,
                    };
                    value = Some(response.value);
                    last_error = None;
                    break;
                }
                Err(error) if error.is_response_lost() => {
                    // Repeated unchanged on the next iteration, if the policy allows one.
                    record.attempts.push(Attempt::ResponseLost);
                    record.result = StepResult::Unresolved;
                    last_error = Some(error);
                }
                Err(error) => {
                    record.attempts.push(match error.api() {
                        Some(failure) => {
                            let document = failure.error().ok();
                            Attempt::Rejected {
                                status: failure.status().as_u16(),
                                error_type: document.map(|d| d.kind.clone()),
                                code: document.and_then(|d| d.code.clone()),
                                request_id: failure.meta().request_id.clone(),
                            }
                        }
                        None => Attempt::Failed,
                    });
                    record.result = if error.api().is_some() {
                        StepResult::Rejected
                    } else {
                        StepResult::Failed
                    };
                    last_error = Some(error);
                    break;
                }
            }
        }
        if value.is_none() {
            self.error = last_error;
        }
        self.writes.push(record);
        value
    }
}

/// What identifies a request for the ledger: method, path and query, `Stripe-Account`, body.
fn request_identity<T>(prepared: &Prepared<T>) -> Vec<u8> {
    let request = prepared.request();
    let mut identity = Vec::new();
    identity.extend_from_slice(request.method().as_str().as_bytes());
    identity.push(b' ');
    identity.extend_from_slice(request.url().path().as_bytes());
    if let Some(query) = request.url().query() {
        identity.push(b'?');
        identity.extend_from_slice(query.as_bytes());
    }
    identity.push(b'\n');
    for value in request.headers().get_all("stripe-account") {
        identity.extend_from_slice(value.as_bytes());
    }
    identity.push(b'\n');
    if let Some(body) = request.body().and_then(|body| body.as_bytes()) {
        identity.extend_from_slice(body);
    }
    identity
}
