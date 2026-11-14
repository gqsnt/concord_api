//! Text that is sent on the wire and shown nowhere else.

use std::fmt;
use std::sync::Arc;

/// A key, a token or a password.
///
/// You can place it in a request. Concord's `Display` and `Debug` never print it: not those
/// of a client, a call or an error, and not a log line. One method is the exception:
/// [`Error::details`](crate::Error::details), which you call to see everything an error
/// holds, prints what the server sent back (its headers, its problem or the start of its
/// body, and at most 1 KiB of a reader's message), so it shows any secret that the server
/// echoed back. (A secret that a description places in a query or in a path becomes part of the
/// URL. Concord never prints a URL, but a `reqwest::Response` that you get from
/// `send_raw()` shows its URL in `{:?}`.)
/// A clone is the same secret: [`replace`](Secret::replace) on one changes them all.
///
/// A secret or an optional secret can be placed in a request; a list of secrets cannot.
///
/// ```
/// # use concord_core as concord;
/// use concord::Secret;
///
/// let key = Secret::new("first-key");
/// let held_by_a_client = key.clone();
/// assert_eq!(format!("{key:?}"), "Secret(…)");
///
/// key.replace("second-key");                      // a new key, while the program runs
/// assert_eq!(&*held_by_a_client.expose(), "second-key");
/// ```
#[derive(Clone)]
pub struct Secret(Arc<std::sync::RwLock<Arc<str>>>);

impl Secret {
    /// Creates a secret from its text.
    pub fn new(text: impl Into<String>) -> Secret {
        Secret(Arc::new(std::sync::RwLock::new(Arc::from(text.into()))))
    }

    /// Returns the text as it is now.
    ///
    /// The returned value dereferences to `&str` and keeps that text alive while you hold it.
    /// A later [`replace`](Secret::replace) does not change text that was already exposed.
    pub fn expose(&self) -> Exposed {
        // A poisoned lock still holds a whole text: a writer only swaps one pointer.
        Exposed(match self.0.read() {
            Ok(text) => text.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        })
    }

    /// Changes the text, for every clone of this secret, from now on.
    pub fn replace(&self, text: impl Into<String>) {
        let text: Arc<str> = Arc::from(text.into());
        match self.0.write() {
            Ok(mut slot) => *slot = text,
            Err(poisoned) => *poisoned.into_inner() = text,
        }
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(…)")
    }
}

impl From<&str> for Secret {
    fn from(text: &str) -> Secret {
        Secret::new(text)
    }
}

impl From<&Secret> for Secret {
    /// Returns the same secret: a later `replace` changes both.
    fn from(secret: &Secret) -> Secret {
        secret.clone()
    }
}

impl From<&String> for Secret {
    fn from(text: &String) -> Secret {
        Secret::new(text.as_str())
    }
}

impl From<String> for Secret {
    fn from(text: String) -> Secret {
        Secret::new(text)
    }
}

/// The text of a [`Secret`] at one moment.
///
/// It has no `Display`, and its `Debug` shows no text: `Exposed(..)`.
pub struct Exposed(Arc<str>);

impl fmt::Debug for Exposed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Exposed(..)")
    }
}

impl std::ops::Deref for Exposed {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn c8_3_it_gives_its_text_only_when_asked() {
        let key = Secret::new("s3cr3t");
        assert_eq!(&*key.expose(), "s3cr3t");
        assert_eq!(format!("{key:?}"), "Secret(…)");
        assert_eq!(format!("{:?}", key.expose()), "Exposed(..)");
    }

    #[test]
    fn c8_3_a_replaced_text_reaches_every_clone_and_no_text_already_exposed() {
        let key = Secret::new("old");
        let clone = key.clone();
        let also = Secret::from(&key);
        let held = key.expose();
        key.replace("new");
        assert_eq!(&*clone.expose(), "new");
        assert_eq!(&*also.expose(), "new");
        assert_eq!(&*held, "old");
    }

    #[test]
    fn c8_3_text_becomes_a_secret() {
        for key in [
            Secret::from("k"),
            Secret::from(String::from("k")),
            Secret::from(&String::from("k")),
        ] {
            assert_eq!(&*key.expose(), "k");
        }
    }
}
