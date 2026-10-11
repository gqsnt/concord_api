use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use crate::error::AuthErrorKind;

use super::AuthError;

/// Maximum UTF-8 bytes retained by one generic protected string.
///
/// Built-in credential types apply their tighter protocol-specific limits
/// before retaining a [`SecretString`].
pub const MAX_SECRET_STRING_BYTES: usize = 1_048_576;
/// Maximum raw byte length accepted from one built-in single-string credential.
pub const MAX_CREDENTIAL_SECRET_BYTES: usize = 65_536;
/// Maximum raw byte length of a Basic authentication username.
pub const MAX_BASIC_USERNAME_BYTES: usize = 8_192;
/// Maximum raw byte length of a Basic authentication password.
pub const MAX_BASIC_PASSWORD_BYTES: usize = 32_768;

/// Stable category for one protected credential-construction failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialMaterialErrorKind {
    /// A generic protected string exceeded its hard retained-storage limit.
    SecretStringTooLarge,
    /// A built-in single-string credential exceeded its hard material limit.
    CredentialSecretTooLarge,
    /// A Basic authentication username exceeded its hard material limit.
    BasicUsernameTooLarge,
    /// A Basic authentication username contained the protocol field delimiter.
    BasicUsernameContainsColon,
    /// A Basic authentication password exceeded its hard material limit.
    BasicPasswordTooLarge,
}

/// A value-free protected credential-construction failure.
pub struct CredentialMaterialError {
    kind: CredentialMaterialErrorKind,
}

impl CredentialMaterialError {
    const fn new(kind: CredentialMaterialErrorKind) -> Self {
        Self { kind }
    }

    /// Returns the stable credential-construction failure category.
    #[must_use]
    pub const fn kind(&self) -> CredentialMaterialErrorKind {
        self.kind
    }
}

impl fmt::Display for CredentialMaterialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "credential material construction failed ({:?})",
            self.kind
        )
    }
}

impl fmt::Debug for CredentialMaterialError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CredentialMaterialError")
            .field("kind", &self.kind)
            .finish()
    }
}

impl std::error::Error for CredentialMaterialError {}

impl From<CredentialMaterialError> for AuthError {
    fn from(error: CredentialMaterialError) -> Self {
        let kind = match error.kind() {
            CredentialMaterialErrorKind::BasicUsernameContainsColon => {
                AuthErrorKind::InvalidConfiguration
            }
            CredentialMaterialErrorKind::SecretStringTooLarge
            | CredentialMaterialErrorKind::CredentialSecretTooLarge
            | CredentialMaterialErrorKind::BasicUsernameTooLarge
            | CredentialMaterialErrorKind::BasicPasswordTooLarge => AuthErrorKind::MaterialTooLarge,
        };
        Self::new(kind)
    }
}

/// A string whose ordinary formatting never reveals its contents.
#[derive(Clone)]
pub struct SecretString {
    value: Arc<str>,
}

impl SecretString {
    /// Copies one protected string after validating its hard storage limit.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, CredentialMaterialError> {
        let value = value.as_ref();
        validate_length(
            value.len(),
            MAX_SECRET_STRING_BYTES,
            CredentialMaterialErrorKind::SecretStringTooLarge,
        )?;
        Ok(Self::from_validated_arc(Arc::from(value)))
    }

    fn from_validated_arc(value: Arc<str>) -> Self {
        Self { value }
    }

    /// Explicitly exposes the protected value.
    #[must_use]
    pub fn expose_secret(&self) -> &str {
        &self.value
    }
}

impl TryFrom<String> for SecretString {
    type Error = CredentialMaterialError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        validate_length(
            value.len(),
            MAX_SECRET_STRING_BYTES,
            CredentialMaterialErrorKind::SecretStringTooLarge,
        )?;
        Ok(Self::from_validated_arc(Arc::from(value)))
    }
}

impl TryFrom<&str> for SecretString {
    type Error = CredentialMaterialError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::try_new(value)
    }
}

impl fmt::Display for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretString(<redacted>)")
    }
}

/// Credential material retained by a Concord credential slot.
///
/// Core reads [`expires_at`](Self::expires_at) once, when the material is
/// installed into a slot and before any slot lock is taken.
///
/// # Extension contract
///
/// * **Ownership.** A slot takes ownership of installed material and shares
///   it immutably behind core-owned reference counting; replaced, cleared or
///   rejected material is dropped when its last in-flight reader finishes.
/// * **Concurrency.** Values may be read by overlapping credential operations
///   on different threads, so every method takes `&self`.
/// * **Cancellation.** Methods are synchronous and cannot be preempted.
///   Concord checks operation deadlines around the credential lifecycle;
///   cancellation prevents later Concord work without rolling back completed
///   extension effects.
/// * **Input sensitivity.** Material is sensitive application-owned state:
///   implementations are responsible for safe `Debug`/`Display`, logging,
///   storage, retention and destruction. Concord never formats material.
/// * **Errors.** Methods cannot fail. An expiration instant that has passed
///   only marks the material unusable: core does not send it and handles it
///   like missing material of the owning credential source.
/// * **Resource obligations.** Implementations must bound CPU, memory, maps,
///   queues, tasks, metric cardinality, retries and external I/O and must not
///   assume Concord supplies automatic limits for custom state.
/// * **Negative capabilities.** Material cannot choose its placement, header
///   or origin, cannot select another credential slot, and cannot trigger
///   acquisition, refresh or replay by itself.
#[diagnostic::on_unimplemented(
    message = "[C3-AUTH] `{Self}` is not credential material",
    label = "a credential slot retains this value",
    note = "credential material implements `CredentialMaterial` (`Send + Sync + 'static`, with an optional expiry) and owns its own confidentiality",
    note = "use the built-in `AccessToken`, `ApiKey` or `BasicCredential`, or implement `CredentialMaterial` for a protected type"
)]
pub trait CredentialMaterial: Send + Sync + 'static {
    /// Returns the provider-declared expiration instant, when present.
    fn expires_at(&self) -> Option<Instant> {
        None
    }
}

/// Single-string credential material suitable for bearer, header, or query placement.
///
/// # Extension contract
///
/// * **Ownership.** The returned string is borrowed from the material for the
///   duration of native request construction; core copies it only into the
///   native authentication header or query value of one attempt.
/// * **Concurrency.** `expose_secret` may be called concurrently for
///   overlapping attempts that share the same material.
/// * **Cancellation.** The call is synchronous and cannot be preempted; the
///   deadline and cancellation obligations of [`CredentialMaterial`] apply.
/// * **Input sensitivity.** `expose_secret` is invoked only at the native
///   authentication boundary, but the implementation remains responsible for
///   protecting the value in its own memory, logging, storage, metrics and
///   synchronous work.
/// * **Errors.** The call cannot fail. Core validates the exposed value
///   against the placement's byte limit (65,536 bytes for a single
///   credential) and header syntax, and reports a violation as a value-free
///   authentication failure before dispatch.
/// * **Resource obligations.** Returning the borrowed string must be prompt
///   and side-effect free; the boundedness obligations of
///   [`CredentialMaterial`] continue to apply.
/// * **Negative capabilities.** The secret cannot choose where it is placed,
///   add headers, or observe the request it authenticates.
#[diagnostic::on_unimplemented(
    message = "[C3-AUTH] `{Self}` is not single-secret credential material",
    label = "a bearer, authorization, header or query placement applies exactly one secret string",
    note = "only `SecretCredential` material can be placed as one bearer/authorization/header/query value; Basic username/password material needs a `basic` placement",
    note = "write a `basic` placement for Basic material, or produce material that implements `SecretCredential`"
)]
pub trait SecretCredential: CredentialMaterial {
    /// Explicitly exposes the value at the native authentication boundary.
    fn expose_secret(&self) -> &str;
}

/// Built-in bearer access-token material.
#[derive(Clone)]
pub struct AccessToken {
    token: SecretString,
    expires_at: Option<Instant>,
}

impl AccessToken {
    /// Creates a bounded token without provider-declared expiration.
    pub fn try_new(token: impl AsRef<str>) -> Result<Self, CredentialMaterialError> {
        Self::try_with_optional_expiration(token, None)
    }

    /// Reuses one protected string as a bounded token without copying it.
    pub fn try_from_secret(token: SecretString) -> Result<Self, CredentialMaterialError> {
        Self::try_from_secret_with_optional_expiration(token, None)
    }

    /// Creates a bounded token with one monotonic expiration instant.
    pub fn try_with_expiration(
        token: impl AsRef<str>,
        expires_at: Instant,
    ) -> Result<Self, CredentialMaterialError> {
        Self::try_with_optional_expiration(token, Some(expires_at))
    }

    /// Reuses one protected string as a bounded expiring token without copying it.
    pub fn try_from_secret_with_expiration(
        token: SecretString,
        expires_at: Instant,
    ) -> Result<Self, CredentialMaterialError> {
        Self::try_from_secret_with_optional_expiration(token, Some(expires_at))
    }

    fn try_with_optional_expiration(
        token: impl AsRef<str>,
        expires_at: Option<Instant>,
    ) -> Result<Self, CredentialMaterialError> {
        let token = token.as_ref();
        validate_credential_secret(token)?;
        Ok(Self {
            token: SecretString::try_new(token)?,
            expires_at,
        })
    }

    fn try_from_secret_with_optional_expiration(
        token: SecretString,
        expires_at: Option<Instant>,
    ) -> Result<Self, CredentialMaterialError> {
        validate_credential_secret(token.expose_secret())?;
        Ok(Self { token, expires_at })
    }
}

impl CredentialMaterial for AccessToken {
    fn expires_at(&self) -> Option<Instant> {
        self.expires_at
    }
}

impl SecretCredential for AccessToken {
    fn expose_secret(&self) -> &str {
        self.token.expose_secret()
    }
}

/// Built-in API-key material.
#[derive(Clone)]
pub struct ApiKey {
    value: SecretString,
}

impl ApiKey {
    /// Creates one bounded API key.
    pub fn try_new(value: impl AsRef<str>) -> Result<Self, CredentialMaterialError> {
        let value = value.as_ref();
        validate_credential_secret(value)?;
        Ok(Self {
            value: SecretString::try_new(value)?,
        })
    }

    /// Reuses one protected string as a bounded API key without copying it.
    pub fn try_from_secret(value: SecretString) -> Result<Self, CredentialMaterialError> {
        validate_credential_secret(value.expose_secret())?;
        Ok(Self { value })
    }
}

impl CredentialMaterial for ApiKey {}

impl SecretCredential for ApiKey {
    fn expose_secret(&self) -> &str {
        self.value.expose_secret()
    }
}

/// Built-in atomic Basic username/password material.
#[derive(Clone)]
pub struct BasicCredential {
    username: SecretString,
    password: SecretString,
}

impl BasicCredential {
    /// Creates one bounded atomic Basic credential pair.
    ///
    /// The username cannot contain `:`, which is the Basic protocol delimiter.
    pub fn try_new(
        username: impl AsRef<str>,
        password: impl AsRef<str>,
    ) -> Result<Self, CredentialMaterialError> {
        let username = username.as_ref();
        let password = password.as_ref();
        validate_basic(username, password)?;
        Ok(Self {
            username: SecretString::try_new(username)?,
            password: SecretString::try_new(password)?,
        })
    }

    /// Reuses two protected strings as one bounded atomic Basic credential pair.
    ///
    /// The username cannot contain `:`, which is the Basic protocol delimiter.
    pub fn try_from_secrets(
        username: SecretString,
        password: SecretString,
    ) -> Result<Self, CredentialMaterialError> {
        validate_basic(username.expose_secret(), password.expose_secret())?;
        Ok(Self { username, password })
    }

    pub(crate) fn username(&self) -> &str {
        self.username.expose_secret()
    }

    pub(crate) fn password(&self) -> &str {
        self.password.expose_secret()
    }
}

impl CredentialMaterial for BasicCredential {}

fn validate_credential_secret(value: &str) -> Result<(), CredentialMaterialError> {
    validate_length(
        value.len(),
        MAX_CREDENTIAL_SECRET_BYTES,
        CredentialMaterialErrorKind::CredentialSecretTooLarge,
    )
}

fn validate_basic(username: &str, password: &str) -> Result<(), CredentialMaterialError> {
    let username_length = username.len();
    let password_length = password.len();
    validate_length(
        username_length,
        MAX_BASIC_USERNAME_BYTES,
        CredentialMaterialErrorKind::BasicUsernameTooLarge,
    )?;
    validate_length(
        password_length,
        MAX_BASIC_PASSWORD_BYTES,
        CredentialMaterialErrorKind::BasicPasswordTooLarge,
    )?;
    if username.as_bytes().contains(&b':') {
        return Err(CredentialMaterialError::new(
            CredentialMaterialErrorKind::BasicUsernameContainsColon,
        ));
    }
    username_length
        .checked_add(1)
        .and_then(|length| length.checked_add(password_length))
        .ok_or_else(|| {
            CredentialMaterialError::new(CredentialMaterialErrorKind::CredentialSecretTooLarge)
        })?;
    Ok(())
}

fn validate_length(
    length: usize,
    maximum: usize,
    kind: CredentialMaterialErrorKind,
) -> Result<(), CredentialMaterialError> {
    if length > maximum {
        Err(CredentialMaterialError::new(kind))
    } else {
        Ok(())
    }
}

macro_rules! redacted_material_format {
    ($type:ty, $name:literal) => {
        impl fmt::Display for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!($name, "(<redacted>)"))
            }
        }

        impl fmt::Debug for $type {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(concat!($name, "(<redacted>)"))
            }
        }
    };
}

redacted_material_format!(AccessToken, "AccessToken");
redacted_material_format!(ApiKey, "ApiKey");
redacted_material_format!(BasicCredential, "BasicCredential");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_material_formatting_is_redacted() {
        const SENTINEL: &str = "credential-material-sentinel-5874c229f7b04e59";
        let values: [&dyn fmt::Debug; 4] = [
            &SecretString::try_new(SENTINEL).expect("bounded protected string"),
            &AccessToken::try_new(SENTINEL).expect("bounded access token"),
            &ApiKey::try_new(SENTINEL).expect("bounded API key"),
            &BasicCredential::try_new(SENTINEL, SENTINEL).expect("bounded Basic credential"),
        ];
        for value in values {
            assert!(!format!("{value:?}").contains(SENTINEL));
        }
    }

    #[test]
    fn secret_string_clone_shares_storage() {
        let secret = SecretString::try_new("shared-secret").expect("bounded protected string");
        let clone = secret.clone();

        assert!(Arc::ptr_eq(&secret.value, &clone.value));
    }

    #[test]
    fn protected_and_built_in_material_limits_reject_first_excess_without_values() {
        const FIRST_EXCESS_SENTINEL: &str = "\u{7f}";

        let protected = "p".repeat(MAX_SECRET_STRING_BYTES);
        assert!(SecretString::try_new(&protected).is_ok());
        let protected_excess = format!("{protected}{FIRST_EXCESS_SENTINEL}");
        let error =
            SecretString::try_new(&protected_excess).expect_err("first protected-string excess");
        assert_eq!(
            error.kind(),
            CredentialMaterialErrorKind::SecretStringTooLarge
        );
        assert!(!error.to_string().contains(FIRST_EXCESS_SENTINEL));
        assert!(!format!("{error:?}").contains(FIRST_EXCESS_SENTINEL));

        let credential = "c".repeat(MAX_CREDENTIAL_SECRET_BYTES);
        assert!(ApiKey::try_new(&credential).is_ok());
        assert!(AccessToken::try_new(&credential).is_ok());
        let credential_excess = format!("{credential}{FIRST_EXCESS_SENTINEL}");
        for error in [
            ApiKey::try_new(&credential_excess).expect_err("first API-key excess"),
            AccessToken::try_new(&credential_excess).expect_err("first access-token excess"),
        ] {
            assert_eq!(
                error.kind(),
                CredentialMaterialErrorKind::CredentialSecretTooLarge
            );
            assert!(!error.to_string().contains(FIRST_EXCESS_SENTINEL));
            assert!(!format!("{error:?}").contains(FIRST_EXCESS_SENTINEL));
        }
    }

    #[test]
    fn basic_material_exact_limits_and_first_excess_are_atomic_and_redacted() {
        const FIRST_EXCESS_SENTINEL: &str = "\u{7f}";

        let username = "u".repeat(MAX_BASIC_USERNAME_BYTES);
        let password = "p".repeat(MAX_BASIC_PASSWORD_BYTES);
        BasicCredential::try_new(&username, &password).expect("exact Basic component limits");

        let username_excess = format!("{username}{FIRST_EXCESS_SENTINEL}");
        let error = BasicCredential::try_new(&username_excess, &password)
            .expect_err("first Basic username excess");
        assert_eq!(
            error.kind(),
            CredentialMaterialErrorKind::BasicUsernameTooLarge
        );
        assert!(!format!("{error:?}").contains(FIRST_EXCESS_SENTINEL));

        let password_excess = format!("{password}{FIRST_EXCESS_SENTINEL}");
        let error = BasicCredential::try_new(&username, &password_excess)
            .expect_err("first Basic password excess");
        assert_eq!(
            error.kind(),
            CredentialMaterialErrorKind::BasicPasswordTooLarge
        );
        assert!(!format!("{error:?}").contains(FIRST_EXCESS_SENTINEL));
    }

    #[test]
    fn basic_username_rejects_every_colon_position_and_password_colons_remain_valid() {
        for username in [":alice", "ali:ce", "alice:"] {
            let error = BasicCredential::try_new(username, "password")
                .expect_err("Basic username delimiter must fail");
            assert_eq!(
                error.kind(),
                CredentialMaterialErrorKind::BasicUsernameContainsColon
            );
            assert_eq!(
                AuthError::from(error).kind(),
                AuthErrorKind::InvalidConfiguration
            );

            let username = SecretString::try_new(username).expect("bounded protected username");
            let password = SecretString::try_new("password").expect("bounded password");
            let error = BasicCredential::try_from_secrets(username, password)
                .expect_err("protected Basic username delimiter must fail");
            assert_eq!(
                error.kind(),
                CredentialMaterialErrorKind::BasicUsernameContainsColon
            );
        }

        BasicCredential::try_new("alice", "prefix:password:tail")
            .expect("Basic passwords may contain colons");
    }
}
