//! The `multipart` format on its own: exact wire bytes for `multipart/form-data` and
//! `multipart/related`, the boundary choice when parts contain the first candidates, every
//! refusal (with a `Display`/`Debug` that never shows part bytes), and the redacted `Debug` of
//! the body. The declared-endpoint behaviour is in `crates/concord/tests/multipart.rs`.
#![cfg(feature = "multipart")]

use concord_codecs::multipart::{Multipart, MultipartBody, Part};
use concord_core::bytes::Bytes;
use concord_core::{CodecError, Encode};

const SECRET: &str = "SECRET-7f3a91";

/// The body bytes and the runtime media type.
fn encode(body: MultipartBody) -> (Vec<u8>, String) {
    let encoded = Multipart::encode_body(body).expect("encodes");
    let (body, media) = encoded.into_parts();
    let bytes = body.as_bytes().expect("a body of known bytes").to_vec();
    (
        bytes,
        media
            .expect("a runtime media type")
            .to_str()
            .unwrap()
            .to_owned(),
    )
}

fn refuse(body: MultipartBody) -> CodecError {
    match Multipart::encode_body(body) {
        Ok(_) => panic!("encoded, expected a refusal"),
        Err(error) => error,
    }
}

/// Every occurrence of `needle` in `haystack`.
fn count(haystack: &[u8], needle: &str) -> usize {
    haystack
        .windows(needle.len())
        .filter(|w| *w == needle.as_bytes())
        .count()
}

#[test]
fn form_data_wire_bytes_are_exact() {
    let content: &[u8] = b"\x00\xff\r\n--not-a-boundary\r\n";
    let (bytes, media) = encode(
        MultipartBody::form_data()
            .text("purpose", "batch")
            .text("caf\u{e9} \u{6771}\u{4eac}", "\u{1f6f0}")
            .file(
                "file",
                "r\u{e9}sum\u{e9}.pdf",
                "application/pdf",
                content.to_vec(),
            ),
    );
    assert_eq!(media, "multipart/form-data; boundary=concord-boundary-0");
    let mut expected = "--concord-boundary-0\r\n\
         Content-Disposition: form-data; name=\"purpose\"\r\n\
         \r\n\
         batch\r\n\
         --concord-boundary-0\r\n\
         Content-Disposition: form-data; name=\"caf\u{e9} \u{6771}\u{4eac}\"\r\n\
         \r\n\
         \u{1f6f0}\r\n\
         --concord-boundary-0\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"r\u{e9}sum\u{e9}.pdf\"\r\n\
         Content-Type: application/pdf\r\n\
         \r\n"
        .as_bytes()
        .to_vec();
    expected.extend_from_slice(content);
    expected.extend_from_slice(b"\r\n--concord-boundary-0--\r\n");
    assert_eq!(bytes, expected);
}

#[test]
fn an_empty_form_is_the_close_delimiter_alone() {
    let (bytes, media) = encode(MultipartBody::form_data());
    assert_eq!(media, "multipart/form-data; boundary=concord-boundary-0");
    assert_eq!(bytes, b"--concord-boundary-0--\r\n");
}

#[test]
fn related_wire_bytes_are_exact_with_and_without_type() {
    let metadata = br#"{"name":"a.bin"}"#;
    let media_bytes: &[u8] = b"\x00\x01binary\r\n";
    let parts = |body: MultipartBody| {
        body.part(
            Part::new(&metadata[..]).header("Content-Type", "application/json; charset=UTF-8"),
        )
        .part(
            Part::new(media_bytes.to_vec())
                .header("Content-Type", "application/octet-stream")
                .header("Content-ID", "<media@example.test>"),
        )
    };
    let mut expected = b"--concord-boundary-0\r\n\
        Content-Type: application/json; charset=UTF-8\r\n\
        \r\n\
        {\"name\":\"a.bin\"}\r\n\
        --concord-boundary-0\r\n\
        Content-Type: application/octet-stream\r\n\
        Content-ID: <media@example.test>\r\n\
        \r\n"
        .to_vec();
    expected.extend_from_slice(media_bytes);
    expected.extend_from_slice(b"\r\n--concord-boundary-0--\r\n");

    let (bytes, media) = encode(parts(MultipartBody::related()));
    assert_eq!(media, "multipart/related; boundary=concord-boundary-0");
    assert_eq!(bytes, expected);

    let (bytes, media) = encode(parts(MultipartBody::related_with_type("application/json")));
    assert_eq!(
        media,
        "multipart/related; boundary=concord-boundary-0; type=\"application/json\""
    );
    assert_eq!(bytes, expected);
}

#[test]
fn a_part_without_headers_has_an_empty_header_block() {
    let (bytes, _) = encode(MultipartBody::related().part(Part::new("root")));
    assert_eq!(
        bytes,
        b"--concord-boundary-0\r\n\r\nroot\r\n--concord-boundary-0--\r\n"
    );
}

#[test]
fn a_boundary_found_in_content_is_skipped() {
    // The file contains the first two candidates, each as a would-be delimiter.
    let content = b"a\r\n--concord-boundary-0\r\nb\r\n--concord-boundary-1--\r\n".to_vec();
    let (bytes, media) = encode(MultipartBody::form_data().file(
        "f",
        "x.bin",
        "application/octet-stream",
        content.clone(),
    ));
    assert_eq!(media, "multipart/form-data; boundary=concord-boundary-2");
    let mut expected = b"--concord-boundary-2\r\n\
        Content-Disposition: form-data; name=\"f\"; filename=\"x.bin\"\r\n\
        Content-Type: application/octet-stream\r\n\
        \r\n"
        .to_vec();
    expected.extend_from_slice(&content);
    expected.extend_from_slice(b"\r\n--concord-boundary-2--\r\n");
    assert_eq!(bytes, expected);
    // The chosen boundary occurs only in the two delimiters.
    assert_eq!(count(&bytes, "concord-boundary-2"), 2);
}

#[test]
fn a_boundary_found_in_part_headers_is_skipped() {
    // In a field name, a file name and a header value: each part's header lines are scanned.
    let (_, media) = encode(
        MultipartBody::form_data()
            .text("concord-boundary-0", "v")
            .file("f", "concord-boundary-1.bin", "text/plain", "x"),
    );
    assert_eq!(media, "multipart/form-data; boundary=concord-boundary-2");
    let (_, media) = encode(
        MultipartBody::related()
            .part(Part::new("x").header("Content-ID", "<concord-boundary-0>"))
            .part(Part::new("y").header("X-concord-boundary-1", "1")),
    );
    assert_eq!(media, "multipart/related; boundary=concord-boundary-2");
}

#[test]
fn candidates_are_ruled_out_across_parts_and_by_longer_numbers() {
    // `concord-boundary-10` contains candidate 1 as well; `-03` only candidate 0.
    let (bytes, media) = encode(
        MultipartBody::related()
            .part(Part::new("concord-boundary-03"))
            .part(Part::new("concord-boundary-10 concord-boundary-2")),
    );
    assert_eq!(media, "multipart/related; boundary=concord-boundary-3");
    assert_eq!(count(&bytes, "concord-boundary-3"), 3);
}

#[test]
fn a_part_holding_many_candidates_still_gets_a_free_boundary() {
    let mut content = Vec::new();
    for n in (0..=500).rev() {
        content.extend_from_slice(format!("--concord-boundary-{n}\r\n").as_bytes());
    }
    let (bytes, media) = encode(MultipartBody::related().part(Part::new(content)));
    assert_eq!(media, "multipart/related; boundary=concord-boundary-501");
    assert_eq!(count(&bytes, "concord-boundary-501"), 2);
}

#[test]
fn the_same_parts_always_get_the_same_bytes() {
    let body = || MultipartBody::form_data().text("a", "concord-boundary-0");
    assert_eq!(encode(body()), encode(body()));
    // `Clone` encodes like the original.
    let original = body();
    assert_eq!(encode(original.clone()), encode(original));
}

/// The label, the 1-based part position (if any), and no secret in `Display` or `Debug`.
fn assert_refused(body: MultipartBody, label: &str, part: Option<u64>) {
    let error = refuse(body);
    assert_eq!(error.label(), label);
    assert_eq!(error.get("part"), part);
    for shown in [error.to_string(), format!("{error:?}")] {
        assert!(!shown.contains(SECRET), "{shown}");
        assert!(!shown.contains("SECRET"), "{shown}");
    }
}

#[test]
fn names_and_file_names_that_cannot_be_quoted_are_refused() {
    const NAME: &str =
        "a multipart field name contains a control character, a quote or a backslash";
    const FILE: &str = "a multipart file name contains a control character, a quote or a backslash";
    for bad in ["\r", "\n", "\0", "\"", "\\", "\x01", "\x1f", "\x7f"] {
        let name = format!("{SECRET}{bad}{SECRET}");
        assert_refused(
            MultipartBody::form_data()
                .text("ok", "v")
                .text(name.clone(), SECRET),
            NAME,
            Some(2),
        );
        assert_refused(
            MultipartBody::form_data().file(name.clone(), "a.txt", "text/plain", SECRET),
            NAME,
            Some(1),
        );
        assert_refused(
            MultipartBody::form_data()
                .text("ok", "v")
                .text("ok", "v")
                .file("f", name, "text/plain", SECRET),
            FILE,
            Some(3),
        );
    }
}

#[test]
fn tab_and_non_ascii_names_are_written_as_they_are() {
    let (bytes, _) = encode(MultipartBody::form_data().file(
        "a\tb",
        "\u{6570}\u{636e}\t1.csv",
        "text/csv;\tcharset=utf-8",
        "x",
    ));
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        text.contains(
            "Content-Disposition: form-data; name=\"a\tb\"; filename=\"\u{6570}\u{636e}\t1.csv\"\r\n\
             Content-Type: text/csv;\tcharset=utf-8\r\n"
        ),
        "{text}"
    );
}

#[test]
fn header_names_that_are_not_tokens_are_refused() {
    const LABEL: &str = "a multipart part header name is not a token";
    for bad in [
        "",
        "Content Type",
        "Content-Type:",
        "X-\r\nInjected",
        "caf\u{e9}",
    ] {
        assert_refused(
            MultipartBody::related()
                .part(Part::new(SECRET))
                .part(Part::new(SECRET).header(bad, SECRET)),
            LABEL,
            Some(2),
        );
    }
}

#[test]
fn header_values_with_control_characters_are_refused() {
    const LABEL: &str = "a multipart part header value contains a control character";
    for bad in ["\r", "\n", "\r\nX-Injected: 1", "\0", "\x1b", "\x7f"] {
        let value = format!("text/plain{bad}{SECRET}");
        assert_refused(
            MultipartBody::related().part(Part::new(SECRET).header("Content-Type", value.clone())),
            LABEL,
            Some(1),
        );
        // A file's content type is a header value too.
        assert_refused(
            MultipartBody::form_data().file("f", "a.txt", value, SECRET),
            LABEL,
            Some(1),
        );
    }
}

#[test]
fn an_empty_related_body_is_refused() {
    assert_refused(
        MultipartBody::related(),
        "a multipart/related body has no part",
        None,
    );
    assert_refused(
        MultipartBody::related_with_type("application/json"),
        "a multipart/related body has no part",
        None,
    );
}

#[test]
fn a_related_type_with_parameters_or_not_a_media_type_is_refused() {
    const LABEL: &str = "the multipart/related type is not a media type without parameters";
    for bad in [
        "application/json; charset=utf-8",
        "application",
        "application/",
        "/json",
        "a/b/c",
        "application/\"json\"",
        "application/json\r\nX-Injected: 1",
    ] {
        assert_refused(
            MultipartBody::related_with_type(bad).part(Part::new(SECRET)),
            LABEL,
            None,
        );
    }
    let (_, media) = encode(
        MultipartBody::related_with_type("application/vnd.example+json").part(Part::new("{}")),
    );
    assert_eq!(
        media,
        "multipart/related; boundary=concord-boundary-0; type=\"application/vnd.example+json\""
    );
}

#[test]
fn form_data_fields_in_a_related_body_are_refused() {
    const LABEL: &str = "a form-data field is in a multipart/related body";
    assert_refused(
        MultipartBody::related()
            .part(Part::new(SECRET))
            .text(SECRET, SECRET),
        LABEL,
        Some(2),
    );
    assert_refused(
        MultipartBody::related().file("f", SECRET, "text/plain", SECRET),
        LABEL,
        Some(1),
    );
}

#[test]
fn encode_alone_refuses_because_the_media_type_has_nowhere_to_go() {
    let error = Multipart::encode(MultipartBody::form_data().text("a", "b")).unwrap_err();
    assert_eq!(
        error.label(),
        "a multipart body has a runtime media type; it is encoded with `encode_body`"
    );
    assert_eq!(<Multipart as Encode<MultipartBody>>::CONTENT_TYPE, None);
}

#[test]
fn debug_withholds_names_values_and_content() {
    let body = MultipartBody::form_data()
        .text(format!("{SECRET}-name"), SECRET)
        .file(
            "f",
            format!("{SECRET}.csv"),
            format!("text/{SECRET}"),
            Bytes::from(SECRET),
        )
        .part(Part::new(SECRET).header(SECRET, SECRET));
    let shown = format!("{body:?}");
    assert!(!shown.contains(SECRET), "{shown}");
    assert_eq!(
        shown,
        "MultipartBody { subtype: \"form-data\", parts: [Part { headers: 1, bytes: 13 }, \
         Part { headers: 2, bytes: 13 }, Part { headers: 1, bytes: 13 }] }"
    );
    assert_eq!(
        format!("{:?}", MultipartBody::related_with_type(SECRET)),
        "MultipartBody { subtype: \"related\", parts: [] }"
    );
}
