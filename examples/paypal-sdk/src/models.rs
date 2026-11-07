//! Request and response models of the slice: identifiers, money, the order request, the order
//! and the capture. Constructors validate what the SDK sends; decoded responses keep what PayPal
//! sent (unknown enumeration values as `Other`, unknown fields ignored).

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

use crate::Error;

/// Up to this many purchase units per order (Orders v2 `order_request.purchase_units`).
pub const MAX_PURCHASE_UNITS: usize = 10;

fn visible_ascii(s: &str) -> bool {
    s.bytes().all(|b| (0x21..=0x7e).contains(&b))
}

fn bounded_text(s: &str, max: usize) -> bool {
    !s.is_empty() && s.chars().count() <= max && !s.chars().any(char::is_control)
}

/// An Orders v2 order ID (path parameter `id`: 1 to 36 characters of `A-Z` and `0-9`).
///
/// [`OrderId::new`] checks what the SDK will send; an ID decoded from a response is kept as PayPal
/// sent it.
#[derive(Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct OrderId(String);

impl OrderId {
    /// # Errors
    ///
    /// [`Error::Invalid`] unless `id` is 1 to 36 characters of `A-Z` and `0-9`.
    pub fn new(id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        let valid = (1..=36).contains(&id.len())
            && id
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        if valid {
            Ok(Self(id))
        } else {
            Err(Error::Invalid(
                "order ID must be 1 to 36 characters of A-Z and 0-9",
            ))
        }
    }

    /// The ID as sent in paths.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for OrderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OrderId({})", self.0)
    }
}

/// A Payments v2 capture ID. PayPal documents no pattern; the SDK requires 1 to 64 visible ASCII
/// characters so that the value is an unambiguous path segment.
#[derive(Clone, PartialEq, Eq, Hash, Deserialize)]
#[serde(transparent)]
pub struct CaptureId(String);

impl CaptureId {
    /// # Errors
    ///
    /// [`Error::Invalid`] unless `id` is 1 to 64 visible ASCII characters.
    pub fn new(id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        if (1..=64).contains(&id.len()) && visible_ascii(&id) {
            Ok(Self(id))
        } else {
            Err(Error::Invalid(
                "capture ID must be 1 to 64 visible ASCII characters",
            ))
        }
    }

    /// The ID as sent in paths (Concord percent-encodes it as one segment).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CaptureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for CaptureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CaptureId({})", self.0)
    }
}

/// A `PayPal-Request-Id`: the caller's unique ID for one logical create or capture.
///
/// PayPal stores it for 6 hours by default (up to 72 by arrangement); within that window the same
/// call with the same ID is not performed twice, and a repeated capture answers `200` with the
/// stored result. The SDK repeats a create or a capture only when it carries one. PayPal allows 1
/// to 10000 characters; the SDK also requires visible ASCII (no spaces) so the header is sent
/// byte for byte. Choose a fresh ID per logical operation and keep it until its outcome is known.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// # Errors
    ///
    /// [`Error::Invalid`] unless `id` is 1 to 10000 visible ASCII characters.
    pub fn new(id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        if (1..=10_000).contains(&id.len()) && visible_ascii(&id) {
            Ok(Self(id))
        } else {
            Err(Error::Invalid(
                "PayPal-Request-Id must be 1 to 10000 visible ASCII characters",
            ))
        }
    }

    /// The ID as sent.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RequestId({})", self.0)
    }
}

/// An amount: an ISO-4217 currency code and a decimal string (Orders v2 `money`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    currency_code: String,
    value: String,
}

impl Money {
    /// `currency_code` must be three ASCII capital letters (PayPal: exactly 3 characters; the
    /// letters are this SDK's ISO-4217 rule); `value` must match PayPal's pattern
    /// `-?[0-9]+` or `-?[0-9]*.[0-9]+`, at most 32 characters. Per-currency decimal places are
    /// checked by PayPal (a `422`), not here.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] otherwise.
    pub fn new(currency_code: impl Into<String>, value: impl Into<String>) -> Result<Self, Error> {
        let (currency_code, value) = (currency_code.into(), value.into());
        if currency_code.len() != 3 || !currency_code.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(Error::Invalid(
                "currency code must be three ASCII capital letters",
            ));
        }
        if value.len() > 32 || !decimal(&value) {
            return Err(Error::Invalid(
                "amount value must be a decimal string of at most 32 characters",
            ));
        }
        Ok(Self {
            currency_code,
            value,
        })
    }

    /// The currency code, as sent or received.
    pub fn currency_code(&self) -> &str {
        &self.currency_code
    }

    /// The decimal value, as sent or received.
    pub fn value(&self) -> &str {
        &self.value
    }

    fn is_positive(&self) -> bool {
        !self.value.starts_with('-') && self.value.bytes().any(|b| (b'1'..=b'9').contains(&b))
    }
}

/// PayPal's `money.value` pattern: `^((-?[0-9]+)|(-?([0-9]+)?[.][0-9]+))$`.
fn decimal(value: &str) -> bool {
    let digits = value.strip_prefix('-').unwrap_or(value);
    let all_digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    match digits.split_once('.') {
        None => !digits.is_empty() && all_digits(digits),
        Some((whole, fraction)) => {
            all_digits(whole) && !fraction.is_empty() && all_digits(fraction)
        }
    }
}

/// One purchase unit of a new order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PurchaseUnitRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    reference_id: Option<String>,
    amount: Money,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    custom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    invoice_id: Option<String>,
}

impl PurchaseUnitRequest {
    /// A purchase unit for `amount`, which PayPal requires to be positive.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] if the amount is zero or negative.
    pub fn new(amount: Money) -> Result<Self, Error> {
        if !amount.is_positive() {
            return Err(Error::Invalid("purchase unit amount must be positive"));
        }
        Ok(Self {
            reference_id: None,
            amount,
            description: None,
            custom_id: None,
            invoice_id: None,
        })
    }

    /// `reference_id` (1 to 256 characters): names this unit; PayPal uses `default` when one
    /// unit has none.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] outside 1 to 256 characters or with a control character.
    pub fn reference_id(mut self, id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        if !bounded_text(&id, 256) {
            return Err(Error::Invalid("reference_id must be 1 to 256 characters"));
        }
        self.reference_id = Some(id);
        Ok(self)
    }

    /// `description` (1 to 3000 characters; PayPal keeps the first 127).
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] outside 1 to 3000 characters or with a control character.
    pub fn description(mut self, text: impl Into<String>) -> Result<Self, Error> {
        let text = text.into();
        if !bounded_text(&text, 3000) {
            return Err(Error::Invalid("description must be 1 to 3000 characters"));
        }
        self.description = Some(text);
        Ok(self)
    }

    /// `custom_id` (1 to 255 characters), echoed on the capture.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] outside 1 to 255 characters or with a control character.
    pub fn custom_id(mut self, id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        if !bounded_text(&id, 255) {
            return Err(Error::Invalid("custom_id must be 1 to 255 characters"));
        }
        self.custom_id = Some(id);
        Ok(self)
    }

    /// `invoice_id` (1 to 127 characters; unique per merchant account by default).
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] outside 1 to 127 characters or with a control character.
    pub fn invoice_id(mut self, id: impl Into<String>) -> Result<Self, Error> {
        let id = id.into();
        if !bounded_text(&id, 127) {
            return Err(Error::Invalid("invoice_id must be 1 to 127 characters"));
        }
        self.invoice_id = Some(id);
        Ok(self)
    }
}

/// The body of Create order: intent `CAPTURE` and 1 to 10 purchase units. Payment sources,
/// `AUTHORIZE` intent and experience context are outside this slice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OrderRequest {
    intent: &'static str,
    purchase_units: Vec<PurchaseUnitRequest>,
}

impl OrderRequest {
    /// An order to capture immediately after the buyer approves it.
    ///
    /// # Errors
    ///
    /// [`Error::Invalid`] for no unit or more than [`MAX_PURCHASE_UNITS`], and, when there are
    /// several units, for a unit without a `reference_id` or two units with the same one (an
    /// SDK rule, so that every capture can be attributed to its unit).
    pub fn capture(units: Vec<PurchaseUnitRequest>) -> Result<Self, Error> {
        if units.is_empty() || units.len() > MAX_PURCHASE_UNITS {
            return Err(Error::Invalid("an order has 1 to 10 purchase units"));
        }
        if units.len() > 1 {
            let mut seen = HashSet::new();
            let distinct = units
                .iter()
                .all(|u| u.reference_id.as_ref().is_some_and(|id| seen.insert(id)));
            if !distinct {
                return Err(Error::Invalid(
                    "several purchase units need distinct reference IDs",
                ));
            }
        }
        Ok(Self {
            intent: "CAPTURE",
            purchase_units: units,
        })
    }
}

/// The body of Capture payment for order: the empty object. A payment source is outside this
/// slice.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct CaptureRequest {}

impl CaptureRequest {
    /// `{}`: capture with the payment source the buyer approved.
    pub fn new() -> Self {
        Self {}
    }
}

macro_rules! provider_enum {
    ($(#[$doc:meta])* $name:ident { $($(#[$vdoc:meta])* $variant:ident = $wire:literal,)+ }) => {
        $(#[$doc])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vdoc])* $variant,)+
            /// A value this SDK does not know, kept as sent.
            Other(String),
        }

        impl $name {
            /// The value as PayPal spells it.
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Other(other) => other,
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                Ok(match value.as_str() {
                    $($wire => Self::$variant,)+
                    _ => Self::Other(value),
                })
            }
        }
    };
}

provider_enum! {
    /// Orders v2 `order_status`.
    OrderStatus {
        /// Created with the given context.
        Created = "CREATED",
        /// Saved and persisted; in progress until every unit has a final capture.
        Saved = "SAVED",
        /// The buyer approved the payment.
        Approved = "APPROVED",
        /// Every purchase unit is voided.
        Voided = "VOIDED",
        /// The intent is complete; check each capture's status before fulfilling.
        Completed = "COMPLETED",
        /// The payer must act first (follow the `payer-action` link).
        PayerActionRequired = "PAYER_ACTION_REQUIRED",
    }
}

provider_enum! {
    /// Orders v2 `checkout_payment_intent`.
    Intent {
        /// Capture immediately after approval.
        Capture = "CAPTURE",
        /// Authorize now, capture later (outside this slice).
        Authorize = "AUTHORIZE",
    }
}

provider_enum! {
    /// Payments v2 `capture_status` (the schema lists the values without descriptions).
    CaptureStatus {
        /// `COMPLETED`.
        Completed = "COMPLETED",
        /// `DECLINED`.
        Declined = "DECLINED",
        /// `PARTIALLY_REFUNDED`.
        PartiallyRefunded = "PARTIALLY_REFUNDED",
        /// `PENDING`: see `status_details.reason`.
        Pending = "PENDING",
        /// `REFUNDED`.
        Refunded = "REFUNDED",
        /// `FAILED`.
        Failed = "FAILED",
    }
}

/// A HATEOAS link (`href`, `rel`, `method`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Link {
    /// The target URL. Never followed by this SDK.
    pub href: String,
    /// The relation (`self`, `approve`, `payer-action`, `capture`, `up`, ...).
    pub rel: String,
    /// The HTTP method, when PayPal states one.
    #[serde(default)]
    pub method: Option<String>,
}

fn link<'a>(links: &'a [Link], rel: &str) -> Option<&'a str> {
    links.iter().find(|l| l.rel == rel).map(|l| l.href.as_str())
}

/// An order (Orders v2), as Create order, Show order details and Capture payment for order
/// return it with `Prefer: return=representation`. Payer and payment-source details are not
/// decoded (they are personal data outside this slice).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Order {
    /// The order ID.
    pub id: OrderId,
    /// The order status.
    pub status: OrderStatus,
    /// The intent, when returned.
    #[serde(default)]
    pub intent: Option<Intent>,
    /// The purchase units, with their payments once captured.
    #[serde(default)]
    pub purchase_units: Vec<PurchaseUnit>,
    /// HATEOAS links.
    #[serde(default)]
    pub links: Vec<Link>,
    /// RFC 3339 creation time, as sent.
    #[serde(default)]
    pub create_time: Option<String>,
    /// RFC 3339 update time, as sent.
    #[serde(default)]
    pub update_time: Option<String>,
}

impl Order {
    /// Where the buyer approves the order: the `approve` link, or `payer-action` when PayPal
    /// returns that relation instead. The approval UI itself is outside this SDK.
    pub fn approval_url(&self) -> Option<&str> {
        link(&self.links, "approve").or_else(|| link(&self.links, "payer-action"))
    }

    /// Every capture of every purchase unit, in order.
    pub fn captures(&self) -> impl Iterator<Item = &Capture> {
        self.purchase_units
            .iter()
            .filter_map(|u| u.payments.as_ref())
            .flat_map(|p| p.captures.iter())
    }
}

/// A purchase unit of a returned order.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct PurchaseUnit {
    /// The unit's reference ID (`default` for a single unit created without one).
    #[serde(default)]
    pub reference_id: Option<String>,
    /// The unit's amount.
    #[serde(default)]
    pub amount: Option<Money>,
    /// Payments made for this unit (present once captured).
    #[serde(default)]
    pub payments: Option<Payments>,
}

/// The payments of a purchase unit. Authorizations and refunds are outside this slice.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Payments {
    /// The captures.
    #[serde(default)]
    pub captures: Vec<Capture>,
}

/// A captured payment (Payments v2), as embedded in an order or returned by Show captured
/// payment details. Seller protection and the receivable breakdown are not decoded.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Capture {
    /// The capture ID.
    pub id: CaptureId,
    /// The capture status.
    pub status: CaptureStatus,
    /// Why the status is what it is (`PENDING`, ...), when PayPal says.
    #[serde(default)]
    pub status_details: Option<StatusDetails>,
    /// The captured amount.
    #[serde(default)]
    pub amount: Option<Money>,
    /// Whether this is the final capture for the authorization.
    #[serde(default)]
    pub final_capture: Option<bool>,
    /// The unit's `invoice_id`, echoed.
    #[serde(default)]
    pub invoice_id: Option<String>,
    /// The unit's `custom_id`, echoed.
    #[serde(default)]
    pub custom_id: Option<String>,
    /// HATEOAS links (`self`, `refund`, `up`).
    #[serde(default)]
    pub links: Vec<Link>,
    /// RFC 3339 creation time, as sent.
    #[serde(default)]
    pub create_time: Option<String>,
    /// RFC 3339 update time, as sent.
    #[serde(default)]
    pub update_time: Option<String>,
    #[serde(default)]
    supplementary_data: Option<Supplementary>,
}

impl Capture {
    /// The order this capture belongs to (`supplementary_data.related_ids.order_id`), when
    /// PayPal includes it (Show captured payment details does).
    pub fn order_id(&self) -> Option<&OrderId> {
        self.supplementary_data
            .as_ref()
            .and_then(|s| s.related_ids.as_ref())
            .and_then(|r| r.order_id.as_ref())
    }
}

/// `status_details` of a capture.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct StatusDetails {
    /// The reason code (`PENDING_REVIEW`, `ECHECK`, ...), as sent.
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct Supplementary {
    #[serde(default)]
    related_ids: Option<RelatedIds>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
struct RelatedIds {
    #[serde(default)]
    order_id: Option<OrderId>,
}
