//! Documentation for the common methods of each generated call.
use super::TokenStream;

pub(super) fn send() -> TokenStream {
    emit! {
        /// Send the call. On a paged endpoint this returns one page.
        /// Preparation starts on the first poll; only the value is kept.
        ///
        /// # Panics
        ///
        /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
        /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
    }
}

pub(super) fn send_full() -> TokenStream {
    emit! {
        /// Send the call, and keep the status and the headers beside the value.
        ///
        /// The value is in a [`concord::Answer`]. Use `value()` to borrow it,
        /// or `into_parts()` to take the status, headers, and value.
        /// Prepares once on the first poll, then sends that preparation.
        ///
        /// # Panics
        ///
        /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
        /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
    }
}

pub(super) fn send_raw() -> TokenStream {
    emit! {
        /// Send the call and get the [`reqwest::Response`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Response.html) as it came.
        ///
        /// Concord passes a response with a status of 400 or above to the retry rules. It does not
        /// apply the endpoint's status rules or read the body. On the first poll, prepares exactly as for
        /// `send_full()`, then sends that preparation only as far as the head.
        ///
        /// # Panics
        ///
        /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
        /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
    }
}

pub(super) fn send_if_none_match() -> TokenStream {
    emit! {
        /// Send a conditional GET or HEAD with `If-None-Match`.
        ///
        /// The ETag is written as given: pass it as it arrived, quotes included
        /// (`"\"v1\""`, `W/"v1"`).
        ///
        /// Replaces any `If-None-Match` set for this endpoint before the
        /// `edit_request` edits run, so those edits can replace or remove it.
        /// On success: `None` for a 304 that no status arm matches, `Some(value)` otherwise.
        /// The status and headers are retained in both cases.
        ///
        /// # Panics
        ///
        /// Sending a call needs a Tokio 1.x runtime with its time and IO drivers enabled.
        /// Polling elsewhere panics with Tokio's own message. Building a client or call needs no runtime.
    }
}

pub(super) fn pages() -> TokenStream {
    emit! {
        /// `.send().await` returns one page; `.pages()` yields them all, and `.pages().items()` each item.
    }
}

pub(super) fn with_retry() -> TokenStream {
    emit! {
        /// The retry policy for this call. Accepts a [`concord::Retry`].
    }
}

pub(super) fn with_timeout() -> TokenStream {
    emit! {
        /// The longest the whole call may take, retries and reading included. No limit by default. Accepts a [`std::time::Duration`].
    }
}

pub(super) fn with_idle_timeout() -> TokenStream {
    emit! {
        /// The longest the transport or a body read may be silent. Accepts a [`std::time::Duration`].
        /// Precedence: this call, the endpoint's `idle_timeout`, the client setting, then 30 seconds.
        /// The endpoint's `idle_timeout` still runs once for every send, before
        /// `idempotent` and request construction, even when this call sets the length.
    }
}

pub(super) fn read_response() -> TokenStream {
    emit! {
        /// Read a response that you obtained yourself using this endpoint's reader and status rules. No retry, call timeout, or idle timeout is started; pages are not advanced, and errors report zero attempts.
    }
}

pub(super) fn request() -> TokenStream {
    emit! {
        /// The [`reqwest::Request`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Request.html) that this call would send.
        /// The call can still be sent. This is the request before any middleware (such as a signer).
        /// Refuses a one-shot body without taking it; a wholly repeatable body stays unopened. The call timeout does not start, and neither the endpoint's `idle_timeout` nor its `idempotent` runs.
    }
}

pub(super) fn into_request() -> TokenStream {
    emit! {
        /// The [`reqwest::Request`](https://docs.rs/reqwest/0.13.5/reqwest/struct.Request.html) that this call would send, and the call is used up.
        ///
        /// Needed when the body is a stream that can be read only once.
        /// Takes that one-shot body. This is the request before any middleware.
        /// A repeatable body is exported as one lazy stream, without its Concord retry plan.
        /// The call timeout does not start, and neither `idle_timeout` nor `idempotent`
        /// from the endpoint runs.
    }
}

pub(super) fn edit_request() -> TokenStream {
    emit! {
        /// Change the built request before it enters the middleware chain.
        ///
        /// The changed request is used on every attempt. Several changes run in the order added.
        /// When the body contains a `Body::repeatable` source, an edit sees an unreadable placeholder;
        /// Reading it, removing it or a detected replacement is a Build error.
        /// Wrappers and look-alikes cannot always be detected; edits must leave the body alone.
        /// If building succeeds, each attempt receives the declared repeatable body as a fresh stream.
        /// A header value marked sensitive is left out of the same-request comparison
        /// for pages, including when this edit sets that flag.
        /// If an edit changes the URL, its secret marks are no longer known;
        /// page comparison uses that URL as it is, without its fragment.
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, process::Command};

    #[test]
    fn authored_type_names_render_as_public_signatures() -> Result<(), Box<dyn std::error::Error>> {
        let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repo = package.parent().ok_or("missing workspace directory")?;
        let work = repo.join("target/call-hygiene-doc");
        fs::create_dir_all(work.join("src"))?;
        fs::copy(
            package.join("tests/accepted/authored_type_names.rs"),
            work.join("src/lib.rs"),
        )?;
        fs::write(
            work.join("Cargo.toml"),
            format!(
                r#"[package]
name = "call_hygiene_doc"
version = "0.0.0"
edition = "2021"
[dependencies]
concord = {{ package = "concord-api", path = {:?} }}
[workspace]
"#,
                repo.join("concord-api")
            ),
        )?;
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .current_dir(&work)
            .args(["doc", "--offline", "--no-deps", "--document-private-items"])
            .env("CARGO_TARGET_DIR", work.join("target"))
            .output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let page =
            fs::read_to_string(work.join("target/doc/call_hygiene_doc/hygiene/struct.Call.html"))?;
        for (method, text) in [
            ("send", "send(self) -> Sending<E::Value, String>"),
            (
                "send_full",
                "send_full(self) -> Sending<Answer<E::Value>, String>",
            ),
            ("send_raw", "send_raw(self) -> Sending<Response, String>"),
            (
                "send_if_none_match",
                "send_if_none_match(self, etag: &str) -> Sending<Answer<Option<E::Value>>, String>",
            ),
            ("pages", "pages(self) -> Pages<E::Value, String>"),
            (
                "with_retry",
                "with_retry(self, retry: Retry<String>) -> Self",
            ),
        ] {
            let signature = page
                .split_once(&format!("id=\"method.{method}\""))
                .and_then(|(_, after)| after.split_once("<h4"))
                .and_then(|(_, header)| header.split_once("</h4>"))
                .map(|(header, _)| plain_text(&format!("<h4{header}")))
                .ok_or_else(|| format!("missing signature for {method}"))?;
            let expected: String = text.split_whitespace().collect();
            // The macro's type resolves outside the alias's lexical scope.
            let resolved = signature.replace("<()as__Types>::__Problem", "String");
            assert!(resolved.contains(&expected), "{method}: {signature}");
        }
        Ok(())
    }

    fn plain_text(html: &str) -> String {
        let mut in_tag = false;
        html.chars()
            .filter(|ch| match ch {
                '<' => {
                    in_tag = true;
                    false
                }
                '>' => {
                    in_tag = false;
                    false
                }
                _ => !in_tag,
            })
            .collect::<String>()
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&")
            .split_whitespace()
            .collect::<String>()
            .replace(",)", ")")
    }
}
