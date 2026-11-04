//! The export's representation: a small CSV format implementing Concord's public [`Decode`]
//! contract, chosen by this crate (the SDK's declared endpoints are JSON).

use concord::bytes::Bytes;
use concord::{CodecError, ContentTypeField, Decode, ResponseHead};

/// The media type the export is requested in (`Accept`) and must be answered in.
pub const TEXT_CSV: &str = "text/csv";

/// One data row of a Catalog export (`id,name`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvRow {
    /// Item identifier, such as `item-001`.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// The CSV of a Catalog export, as a format: `Decode<Vec<CsvRow>>`.
///
/// - **Head** ([`check_content_type`](Decode::check_content_type)): exactly one `Content-Type`
///   whose media type is `text/csv` (any case) and whose `charset`, if given, is `utf-8`.
///   Anything else is refused before the body is read (Concord's
///   [`ErrorKind::Metadata`](concord::ErrorKind::Metadata), the response kept unread).
/// - **Body**: UTF-8 text in lines (`\n` or `\r\n`). Empty lines and lines starting with `#` (the
///   service appends a `# ...` trailer) are skipped. The first other line must be the header
///   `id,name`; every following line is one row of exactly two unquoted fields.
///
/// Every [`CodecError`] is a static label with numeric positions (`line`, `column`, `fields`,
/// `byte`): never the body's text.
#[derive(Debug, Clone, Copy)]
pub struct Csv;

impl Decode<Vec<CsvRow>> for Csv {
    type Context = ();

    const ACCEPT: Option<&'static str> = Some(TEXT_CSV);

    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        let value = match head.content_type() {
            ContentTypeField::One(value) => value,
            ContentTypeField::Missing => {
                return Err(CodecError::new("the response has no Content-Type"));
            }
            ContentTypeField::Repeated => {
                return Err(CodecError::new("the response repeats Content-Type"));
            }
        };
        let Ok(text) = value.to_str() else {
            return Err(CodecError::new(
                "the response Content-Type is not visible ASCII",
            ));
        };
        let mut parts = text.split(';');
        let essence = parts.next().unwrap_or_default().trim();
        if !essence.eq_ignore_ascii_case(TEXT_CSV) {
            return Err(CodecError::new("the response is not text/csv"));
        }
        for parameter in parts {
            let (name, value) = parameter.split_once('=').unwrap_or((parameter, ""));
            if name.trim().eq_ignore_ascii_case("charset")
                && !value.trim().trim_matches('"').eq_ignore_ascii_case("utf-8")
            {
                return Err(CodecError::new("the CSV charset is not utf-8"));
            }
        }
        Ok(())
    }

    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    fn decode_body((): (), body: Bytes) -> Result<Vec<CsvRow>, CodecError> {
        let text = std::str::from_utf8(&body).map_err(|e| {
            CodecError::new("the CSV body is not UTF-8").at("byte", e.valid_up_to() as u64)
        })?;
        let mut header_seen = false;
        let mut rows = Vec::new();
        for (index, line) in text.split('\n').enumerate() {
            let number = index as u64 + 1;
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(column) = line.find('"') {
                return Err(CodecError::new("quoted CSV fields are not supported")
                    .at("line", number)
                    .at("column", column as u64 + 1));
            }
            if !header_seen {
                if line != "id,name" {
                    return Err(
                        CodecError::new("the CSV header is not `id,name`").at("line", number)
                    );
                }
                header_seen = true;
                continue;
            }
            let fields: Vec<&str> = line.split(',').collect();
            match fields.as_slice() {
                [id, name] => rows.push(CsvRow {
                    id: (*id).to_owned(),
                    name: (*name).to_owned(),
                }),
                _ => {
                    return Err(CodecError::new("a CSV row does not have 2 fields")
                        .at("line", number)
                        .at("fields", fields.len() as u64));
                }
            }
        }
        if !header_seen {
            return Err(CodecError::new("the CSV body has no header line"));
        }
        Ok(rows)
    }
}
