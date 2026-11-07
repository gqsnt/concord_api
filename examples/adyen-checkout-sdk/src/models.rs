use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fmt};

// Provider-controlled strings can include secrets; Debug never delegates to them.
macro_rules! redacted_debug {
    ($($ty:ty),+ $(,)?) => {$ (
        impl fmt::Debug for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(concat!(stringify!($ty), "(<redacted>)"))
            }
        }
    )+};
}

/// Amount in minor units, without floating-point conversion.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Amount {
    /// Three-letter ISO currency code; provider validates currency availability.
    pub currency: String,
    /// Minor-unit amount.
    pub value: u64,
}

/// Transaction context for the non-paginated methods list.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodsRequest {
    /// Merchant account used by the sandbox credential.
    pub merchant_account: String,
    /// Amount used to filter available methods.
    pub amount: Amount,
    /// Shopper country, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country_code: Option<String>,
    /// Shopper locale, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shopper_locale: Option<String>,
    /// SDK slice always uses the Web channel.
    pub channel: Web,
}

/// This slice uses the Web channel only.
#[derive(Clone, Copy, Debug, Serialize)]
pub enum Web {
    /// Web checkout.
    Web,
}

/// An available method; unknown provider metadata is preserved.
#[derive(Clone, Deserialize)]
pub struct AvailableMethod {
    /// Provider's method code (for example `scheme` or `ideal`).
    #[serde(rename = "type")]
    pub kind: String,
    /// Localized display name.
    pub name: Option<String>,
    /// Provider configuration, brands and issuer choices.
    #[serde(flatten)]
    pub attributes: BTreeMap<String, Value>,
}

/// A single methods response. Stored/recurring methods are deliberately excluded.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodsResponse {
    /// Available methods (the provider may omit the list when none are available).
    #[serde(default)]
    pub payment_methods: Vec<AvailableMethod>,
}

/// Bounded sandbox inputs. Raw PAN/CVC, stored methods and recurring payments are not modeled.
#[derive(Clone, Serialize)]
#[serde(tag = "type")]
pub enum SandboxMethod {
    /// iDEAL sandbox issuer choice. Select the issuer using the methods response.
    #[serde(rename = "ideal")]
    Ideal {
        /// Sandbox issuer identifier.
        issuer: String,
    },
    /// Adyen's `test_` encrypted card values. Use [`Self::test_card`] to construct.
    #[serde(rename = "scheme")]
    TestCard {
        /// Test card number token.
        #[serde(rename = "encryptedCardNumber")]
        encrypted_card_number: TestToken,
        /// Test expiry month token.
        #[serde(rename = "encryptedExpiryMonth")]
        encrypted_expiry_month: TestToken,
        /// Test expiry year token.
        #[serde(rename = "encryptedExpiryYear")]
        encrypted_expiry_year: TestToken,
        /// Test security code token.
        #[serde(rename = "encryptedSecurityCode")]
        encrypted_security_code: TestToken,
    },
}

/// A sandbox-only encrypted card token, constructed through [`SandboxMethod::test_card`].
#[derive(Clone, Serialize)]
#[serde(transparent)]
pub struct TestToken(String);

impl SandboxMethod {
    /// Accepts only nonempty `test_` prefixed values; never accepts raw card data.
    pub fn test_card(
        number: &str,
        month: &str,
        year: &str,
        cvc: &str,
    ) -> Result<Self, crate::Error> {
        fn token(s: &str) -> Result<TestToken, crate::Error> {
            if !s.starts_with("test_") || s.len() <= 5 || !s.bytes().all(|b| b.is_ascii_graphic()) {
                return Err(crate::Error::InvalidInput(
                    "only sandbox test_ card tokens are accepted",
                ));
            }
            Ok(TestToken(s.into()))
        }
        Ok(Self::TestCard {
            encrypted_card_number: token(number)?,
            encrypted_expiry_month: token(month)?,
            encrypted_expiry_year: token(year)?,
            encrypted_security_code: token(cvc)?,
        })
    }
}

/// A sandbox payment submission. The API key and idempotency key are headers, never body fields.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentRequest {
    /// Merchant account.
    pub merchant_account: String,
    /// Minor-unit amount.
    pub amount: Amount,
    /// Caller-chosen merchant reference for reconciliation.
    pub reference: String,
    /// Caller-owned return location. The SDK never visits this URL.
    pub return_url: String,
    /// Selected sandbox method.
    pub payment_method: SandboxMethod,
    /// Web checkout channel.
    pub channel: Web,
}

/// Caller-obtained continuation details; strings retain provider wire keys without interpretation.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailsRequest {
    /// For a redirect: `redirectResult` from the return URL, decoded by the application once.
    pub details: BTreeMap<String, String>,
    /// Optional opaque flow data if required by the selected action.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_data: Option<String>,
}

impl DetailsRequest {
    /// Creates the redirect continuation, without parsing or visiting the action URL.
    pub fn redirect(result: impl Into<String>) -> Self {
        Self {
            details: BTreeMap::from([("redirectResult".into(), result.into())]),
            payment_data: None,
        }
    }
}

/// A next action. Extra fields preserve unfamiliar action variants for the caller.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    /// Action discriminator, such as `redirect`, `threeDS2` or `qrCode`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Opaque continuation data, if present.
    pub payment_data: Option<String>,
    /// Provider action fields including URL, method, data and paymentMethodType.
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

/// Payment state projected conservatively; unknown states are never treated as authorised.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaymentState {
    /// Caller must process the returned action; its presence takes precedence over resultCode.
    ActionRequired,
    /// Authorised; this says nothing about capture or settlement.
    Authorised,
    /// Refused, cancelled or processing error. See the full response for refusal data.
    Unsuccessful,
    /// Received/Pending: await external reconciliation, not another fresh payment.
    Pending,
    /// Includes partial authorisation, authentication-only results and new provider states.
    Other,
}

/// Shared response for `/payments` and `/payments/details`.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentResponse {
    /// Provider result code, retained exactly even if unknown.
    pub result_code: String,
    /// Payment reference; may be absent until details complete.
    pub psp_reference: Option<String>,
    /// Merchant reference when returned.
    pub merchant_reference: Option<String>,
    /// Further shopper action, never executed automatically.
    pub action: Option<Action>,
    /// Provider refusal explanation; not exposed in Debug.
    pub refusal_reason: Option<String>,
    /// Additional provider fields, preserved for reconciliation.
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}

impl PaymentResponse {
    /// Returns a conservative state without equating an HTTP success with payment success.
    pub fn state(&self) -> PaymentState {
        if self.action.is_some() {
            return PaymentState::ActionRequired;
        }
        match self.result_code.as_str() {
            "Authorised" => PaymentState::Authorised,
            "Refused" | "Cancelled" | "Error" => PaymentState::Unsuccessful,
            "Received" | "Pending" => PaymentState::Pending,
            _ => PaymentState::Other,
        }
    }
}

redacted_debug!(
    Amount,
    MethodsRequest,
    AvailableMethod,
    MethodsResponse,
    SandboxMethod,
    TestToken,
    PaymentRequest,
    DetailsRequest,
    Action,
    PaymentResponse
);
