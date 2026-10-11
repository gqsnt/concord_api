//! Installed credential versions and explicit storage outcomes.
//!
//! A credential slot owns two separate checked counters (AUTH-03):
//!
//! * the **installation intent epoch** advances whenever an explicit
//!   endpoint-backed storage operation is first polled, whenever a provider
//!   leader starts one acquisition, and on every manual `clear`/`replace`;
//! * the **installed version** advances only when material is actually
//!   published into the slot.
//!
//! [`CredentialVersion`] exposes the second counter as an opaque token. It is
//! never used as an intent epoch and never derived from one.

use std::fmt;

/// Opaque identity of one installed credential version in one client's
/// credential store.
///
/// A version is allocated only when material is actually installed (explicit
/// storage that is still the latest-started intent, a manual replacement, or a
/// provider acquisition that was not superseded). Versions are allocated from
/// a checked, never-wrapping counter owned by the credential store; two
/// installations in the same store never share a version. Values from
/// independently built clients are unrelated and must not be compared.
///
/// The type carries no credential material. Its `Debug` output is value-free
/// and does not reveal the counter.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct CredentialVersion(u64);

impl CredentialVersion {
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }
}

impl fmt::Debug for CredentialVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CredentialVersion(..)")
    }
}

/// Result of one explicit endpoint-backed credential storage operation.
///
/// Storage follows **strict latest-started explicit intent**: the first poll
/// of a storage terminal reserves a new installation intent, and a later
/// storage start, manual `clear` or manual `replace` supersedes every earlier
/// pending intent. Applications must examine the outcome. `Superseded` is not
/// a transport failure; the produced material was dropped without being
/// returned or installed, and an automatic new login in response would undo a
/// user's later logout or replacement.
#[must_use = "a storage outcome reports whether the credential was actually installed"]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreOutcome {
    /// The decoded material was installed atomically as a new version while
    /// this operation's intent was still the latest-started intent.
    Installed {
        /// The newly allocated installed version.
        version: CredentialVersion,
    },
    /// A later storage start, manual clear or manual replacement superseded
    /// this operation's intent. Nothing was installed and the material was
    /// dropped.
    Superseded,
}

impl StoreOutcome {
    /// Returns whether material was installed.
    #[must_use]
    pub const fn is_installed(&self) -> bool {
        matches!(self, Self::Installed { .. })
    }

    /// Returns the installed version, or `None` when superseded.
    #[must_use]
    pub const fn installed_version(&self) -> Option<CredentialVersion> {
        match self {
            Self::Installed { version } => Some(*version),
            Self::Superseded => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_debug_is_value_free_and_outcome_accessors_are_exact() {
        let version = CredentialVersion::new(0x5ec2_e7ab_cdef);
        let rendered = format!("{version:?}");
        assert_eq!(rendered, "CredentialVersion(..)");
        assert!(!rendered.contains(&0x5ec2_e7ab_cdef_u64.to_string()));
        assert_eq!(version, CredentialVersion::new(0x5ec2_e7ab_cdef));
        assert_ne!(version, CredentialVersion::new(1));

        let installed = StoreOutcome::Installed { version };
        assert!(installed.is_installed());
        assert_eq!(installed.installed_version(), Some(version));
        assert!(!StoreOutcome::Superseded.is_installed());
        assert_eq!(StoreOutcome::Superseded.installed_version(), None);
        assert_eq!(
            format!("{installed:?}"),
            "Installed { version: CredentialVersion(..) }"
        );
    }
}
