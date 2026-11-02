//! U29 (R09): a `multipart/form-data` upload through reqwest's own `multipart` feature.
//!
//! The endpoint is declared without a body: Concord owns the path, the authorization header and
//! the decoder. The application builds a [`reqwest::multipart::Form`] and hands the prepared
//! request back to reqwest:
//!
//! ```text
//! api.upload(folder).prepare()?.into_parts()          // (client, request, decoder)
//!   → RequestBuilder::from_parts(client, request)
//!       .multipart(form).send().await               // native body, boundary, Content-Type
//!   → decoder.finish(result).await                  // declared 2xx policy + `json` decoding
//! ```
//!
//! Why `from_parts(..).multipart(..)` rather than setting the body on the `Request`: reqwest's
//! builder writes the `Content-Type` with the per-body boundary *and* the `Content-Length` (when
//! every part has a known length) from the same `Form`, through public API only. Setting the body
//! by hand would need reqwest's `stream` feature (`Form::into_stream`) and would send a chunked
//! body, because the form's computed length is not public.
//!
//! This crate is the only one in the workspace that enables reqwest's `multipart` feature; the
//! `concord` crate itself has no multipart policy and does not depend on it.

use reqwest::multipart::{Form, Part};
use serde::Deserialize;

/// What the server answers after an upload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Stored {
    /// Identifier of the stored upload.
    pub id: u64,
    /// Number of files stored.
    pub files: u32,
}

concord::api! {
    /// A small file store. `key` is the full `Authorization` value (e.g. `Bearer ...`).
    pub client Files(#[sensitive] header("authorization") key: String) {
        base "http://files.invalid/v1";

        /// Upload into a folder; the multipart body is attached natively (see [`upload`]).
        POST upload(folder: String) -> Stored { response json; path "folders/{folder}/files"; }
    }
}

/// One file part of an upload.
#[derive(Debug, Clone)]
pub struct FileUpload {
    /// The form field name.
    pub field: String,
    /// The file name sent in `Content-Disposition`.
    pub file_name: String,
    /// The part's media type, e.g. `text/csv`.
    pub content_type: String,
    /// The file content (buffered; see [`Part::stream_with_length`] for streamed content).
    pub content: Vec<u8>,
}

/// Builds a form from file parts and plain text fields, in order (text fields first).
///
/// Field names go through [`Form::percent_encode_attr_chars`]: a name made only of RFC 8187
/// `attr-char`s (letters, digits, `!#$&+-.^_`|~`) is sent as `name="..."`; any other name is sent
/// as `name*=utf-8''<percent-encoded>`, which a recipient decodes unambiguously. reqwest's default
/// (`percent_encode_path_segment`) leaves `\` unescaped inside `name="..."`, where a recipient
/// reads it as a quoted-pair escape (`file\name` arrives as `filename`).
///
/// File names are written by reqwest as a quoted string: `\` and `"` are backslash-escaped,
/// non-ASCII is sent as raw UTF-8, and CR/LF are sent as a backslash followed by the raw
/// character. That last form is never a line break on the wire, but RFC 9110 does not allow CR/LF
/// in a quoted string, so a strict recipient may reject it: sanitize such names first if the
/// server is not known to accept them.
///
/// Fails only for a content type that is not a valid media type.
pub fn file_form(text: &[(&str, &str)], files: Vec<FileUpload>) -> Result<Form, reqwest::Error> {
    let mut form = Form::new().percent_encode_attr_chars();
    for (name, value) in text {
        form = form.text(name.to_string(), value.to_string());
    }
    for file in files {
        let part = Part::bytes(file.content)
            .file_name(file.file_name)
            .mime_str(&file.content_type)?;
        form = form.part(file.field, part);
    }
    Ok(form)
}

/// Uploads `form` into `folder`: declared path, authorization and decoder; native multipart body.
///
/// A non-2xx answer is a [`concord::ErrorKind::Status`] error whose response is still unread
/// ([`concord::Error::into_response`]); a failure to send or to read the answer is a
/// [`concord::ErrorKind::Transport`] error. Both come from [`concord::Decoder::finish`], exactly
/// as for a declared body.
pub async fn upload(api: &Files, folder: &str, form: Form) -> Result<Stored, concord::Error> {
    let (client, request, decoder) = api.upload(folder).prepare()?.into_parts();
    let result = reqwest::RequestBuilder::from_parts(client, request)
        .multipart(form)
        .send()
        .await;
    decoder.finish(result).await
}
