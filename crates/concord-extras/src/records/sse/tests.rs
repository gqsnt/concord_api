use std::error::Error as _;

use super::*;

fn drain(
    decoder: &mut SseDecoder,
    buffer: &mut BytesMut,
    eof: bool,
    events: &mut Vec<ServerEvent>,
) -> Result<(), RecordError> {
    loop {
        let before = buffer.len();
        let event = if eof {
            decoder.decode_eof(buffer)?
        } else {
            decoder.decode(buffer)?
        };
        assert!(
            buffer.len() <= before,
            "a callback must not grow core staging"
        );
        if let Some(event) = event {
            assert!(buffer.len() < before, "a record must consume staged bytes");
            events.push(event);
        } else if buffer.len() == before {
            return Ok(());
        }
    }
}

fn feed(chunks: impl IntoIterator<Item = Vec<u8>>) -> Result<Vec<ServerEvent>, RecordError> {
    let mut decoder = SseDecoder::new(false);
    let mut buffer = BytesMut::new();
    let mut events = Vec::new();
    for chunk in chunks {
        buffer.extend_from_slice(&chunk);
        drain(&mut decoder, &mut buffer, false, &mut events)?;
    }
    drain(&mut decoder, &mut buffer, true, &mut events)?;
    assert!(buffer.is_empty());
    assert_eq!(decoder.buffered_bytes(), 0);
    Ok(events)
}

fn assert_all_splits(input: &[u8], expected: &[ServerEvent]) {
    assert_eq!(feed([input.to_vec()]).expect("one chunk"), expected);
    assert_eq!(
        feed(input.iter().map(|byte| vec![*byte])).expect("one-byte chunks"),
        expected
    );
    for split in 0..=input.len() {
        assert_eq!(
            feed([input[..split].to_vec(), input[split..].to_vec()]).expect("two chunks"),
            expected,
            "source split {split}"
        );
    }
}

fn event(data: &str, event_type: &str, id: &str, retry: Option<u64>) -> ServerEvent {
    ServerEvent {
        data: data.into(),
        event_type: event_type.into(),
        id: id.into(),
        retry: retry.map(Duration::from_millis),
    }
}

#[test]
fn lf_crlf_and_cr_are_chunk_invariant_with_multiline_data_and_empty_events() {
    let input =
        b": first comment\r\nevent: update\rdata: first\r\ndata: second\n\rdata\n\ndata: end\r\r";
    assert_all_splits(
        input,
        &[
            event("first\nsecond", "update", "", None),
            event("", "message", "", None),
            event("end", "message", "", None),
        ],
    );
}

#[test]
fn mixed_line_endings_do_not_invent_or_lose_blank_lines() {
    for ending in ["\n", "\r\n", "\r"] {
        let input = format!("data: a{ending}{ending}data: b{ending}{ending}");
        assert_all_splits(
            input.as_bytes(),
            &[
                event("a", "message", "", None),
                event("b", "message", "", None),
            ],
        );
    }
    assert_all_splits(
        b"data: a\r\n\rdata: b\n\r\n",
        &[
            event("a", "message", "", None),
            event("b", "message", "", None),
        ],
    );
}

#[test]
fn strips_exactly_one_bom_only_at_the_start_and_decodes_unicode_across_splits() {
    let input =
        "\u{feff}data: snowman ☃\ndata: \u{feff}inside\n\n\u{feff}data: ignored\ndata: last\n\n";
    assert_all_splits(
        input.as_bytes(),
        &[
            event("snowman ☃\n\u{feff}inside", "message", "", None),
            event("last", "message", "", None),
        ],
    );
    assert_all_splits(
        "\u{feff}\u{feff}data: ignored\n\ndata: visible\n\n".as_bytes(),
        &[event("visible", "message", "", None)],
    );
    for input in [b"\xef".as_slice(), b"\xef\xbb", BOM] {
        assert_all_splits(input, &[]);
    }
}

#[test]
fn malformed_utf8_uses_replacement_characters_for_all_field_values() {
    let input = b"id: \xf0\x90\x80\nevent: \xff\ndata: \xe2\x82\ndata: \xed\xa0\x80\n\n";
    assert_all_splits(input, &[event("�\n���", "�", "�", None)]);
    assert_all_splits(
        b"data: \xef\xbb\xbf\xff\xc0\xaf\n\n",
        &[event("\u{feff}���", "message", "", None)],
    );
}

#[test]
fn fields_are_literal_and_strip_only_one_space_after_the_first_colon() {
    let input = b"Data: ignored\n data: ignored\nunknown: ignored\n: comment\ndata:  leading\ndata:\ttab\ndata:a:b\ndata\nevent: first\nevent:\n\n";
    assert_all_splits(
        input,
        &[event(" leading\n\ttab\na:b\n", "message", "", None)],
    );
    assert_all_splits(
        b"event: ignored-without-data\n\ndata: x\n\n",
        &[event("x", "message", "", None)],
    );
}

#[test]
fn ids_persist_clear_and_ignore_nul_including_control_only_blocks() {
    let input = b"id: first\n\ndata: a\n\nid: bad\0value\ndata: b\n\nid\n\ndata: c\n\nid: old\nid: last\ndata: d\n\n";
    assert_all_splits(
        input,
        &[
            event("a", "message", "first", None),
            event("b", "message", "first", None),
            event("c", "message", "", None),
            event("d", "message", "last", None),
        ],
    );
}

#[test]
fn retry_accepts_only_representable_nonempty_ascii_decimal_milliseconds() {
    assert_eq!(parse_retry("0"), Some(0));
    assert_eq!(parse_retry("000123"), Some(123));
    assert_eq!(parse_retry("18446744073709551615"), Some(u64::MAX));
    for invalid in [
        "",
        "-1",
        "+1",
        " 1",
        "1 ",
        "1.0",
        "١",
        "１",
        "18446744073709551616",
    ] {
        assert_eq!(parse_retry(invalid), None, "invalid retry {invalid:?}");
    }
    let input = b"retry: 0012\n\ndata: a\n\nretry: -1\nretry:\nretry: 18446744073709551616\ndata: b\n\nretry: 0\ndata: c\n\n";
    assert_all_splits(
        input,
        &[
            event("a", "message", "", Some(12)),
            event("b", "message", "", Some(12)),
            event("c", "message", "", Some(0)),
        ],
    );
}

#[test]
fn eof_discards_unterminated_events_and_does_not_dispatch_again() {
    for tail in [
        "",
        "data: tail",
        "data: tail\n",
        "data: tail\r\n",
        "event: tail\ndata: value\n",
    ] {
        let input = format!("data: complete\n\n{tail}");
        assert_all_splits(input.as_bytes(), &[event("complete", "message", "", None)]);
    }
    let mut decoder = SseDecoder::new(false);
    let mut buffer = BytesMut::from(b"data: complete\n\n".as_slice());
    let mut events = Vec::new();
    drain(&mut decoder, &mut buffer, true, &mut events).expect("EOF records");
    assert_eq!(events, [event("complete", "message", "", None)]);
    assert!(
        decoder
            .decode_eof(&mut buffer)
            .expect("second EOF")
            .is_none()
    );
    assert!(
        decoder
            .decode_eof(&mut buffer)
            .expect("third EOF")
            .is_none()
    );
}

#[test]
fn shared_state_observes_control_only_blocks_and_can_be_initialized_before_decode() {
    let mut decoder = SseDecoder::new(false);
    let state = Arc::clone(&decoder.state);
    {
        let mut state = state.lock().expect("state lock");
        state.id = "prior-connection".into();
        state.retry = Some(Duration::from_millis(45));
    }
    let mut events = Vec::new();
    let mut buffer = BytesMut::from(b"data: inherited\n\nid:\nretry: 7\n\n".as_slice());
    drain(&mut decoder, &mut buffer, false, &mut events).expect("control block");
    assert_eq!(
        events,
        [event("inherited", "message", "prior-connection", Some(45))]
    );
    let state = state.lock().expect("state lock");
    assert!(state.id.is_empty());
    assert_eq!(state.retry, Some(Duration::from_millis(7)));
}

#[test]
fn unfinished_id_does_not_commit_but_a_complete_retry_line_takes_effect_immediately() {
    let mut decoder = SseDecoder::new(false);
    let state = Arc::clone(&decoder.state);
    let mut events = Vec::new();
    let mut buffer = BytesMut::from(b"id: pending\nretry: 3\ndata: unfinished\n".as_slice());
    drain(&mut decoder, &mut buffer, true, &mut events).expect("unfinished block");
    assert!(events.is_empty());
    let state = state.lock().expect("state lock");
    assert!(state.id.is_empty());
    assert_eq!(state.retry, Some(Duration::from_millis(3)));
}

#[test]
fn decoder_owned_storage_reports_accumulated_event_and_pending_metadata() {
    let mut decoder = SseDecoder::new(false);
    let mut events = Vec::new();
    let mut buffer = BytesMut::from(b"data: abc\nevent: update\nid: pending\n".as_slice());
    drain(&mut decoder, &mut buffer, false, &mut events).expect("partial event");
    assert_eq!(decoder.buffered_bytes(), 4 + 6 + 7);
    buffer.extend_from_slice(b"\n");
    drain(&mut decoder, &mut buffer, false, &mut events).expect("dispatch");
    assert_eq!(decoder.buffered_bytes(), 0);
    assert_eq!(events, [event("abc", "update", "pending", None)]);
}

#[test]
fn data_prefix_and_one_optional_space_are_consumed_before_the_line_ends() {
    let mut decoder = SseDecoder::new(false);
    let mut buffer = BytesMut::from(b"data:".as_slice());
    assert!(decoder.decode(&mut buffer).expect("prefix").is_none());
    assert!(buffer.is_empty());
    assert_eq!(decoder.data_prefix, 5);
    assert!(decoder.strip_space_pending);
    buffer.extend_from_slice(b"  payload");
    assert!(decoder.decode(&mut buffer).expect("value").is_none());
    assert_eq!(&buffer[..], b" payload");
    assert_eq!(decoder.data_prefix, 6);
    assert!(!decoder.strip_space_pending);
    buffer.extend_from_slice(b"\n\n");
    let mut events = Vec::new();
    drain(&mut decoder, &mut buffer, false, &mut events).expect("complete event");
    assert_eq!(events, [event(" payload", "message", "", None)]);
    assert_eq!(decoder.data_prefix, 0);
}

#[test]
fn stripped_data_prefix_still_counts_toward_the_raw_line_byte_limit() {
    for prefix in ["data:", "data: "] {
        let value = "x".repeat(MAX_LINE_BYTES - prefix.len());
        let input = format!("{prefix}{value}\n\n");
        let events = feed([input.into_bytes()]).expect("exact raw line bound");
        assert_eq!(events[0].data(), value);
        let input = format!("{prefix}{value}x\n\n");
        assert_eq!(
            feed([input.into_bytes()])
                .expect_err("prefix contributes to raw line size")
                .kind(),
            RecordErrorKind::LimitExceeded
        );
    }
}

#[test]
fn eof_discards_a_data_field_after_its_prefix_has_been_consumed() {
    for input in [b"data:".as_slice(), b"data: ", b"data: partial"] {
        assert_all_splits(input, &[]);
    }
}

#[test]
fn partial_data_prefixes_do_not_turn_unknown_fields_into_control_fields() {
    assert_all_splits(
        b"did: ignored\ndaretry: 1\ndatevent: ignored\ndataid: ignored\nd: ignored\nda\ndat\ndata\n\n",
        &[event("", "message", "", None)],
    );
    let mut decoder = SseDecoder::new(false);
    let mut buffer = BytesMut::new();
    for byte in b"data:" {
        buffer.extend_from_slice(&[*byte]);
        assert!(
            decoder
                .decode(&mut buffer)
                .expect("partial prefix")
                .is_none()
        );
        assert!(buffer.is_empty(), "framing bytes need not accumulate");
    }
    assert_eq!(decoder.data_prefix, 5);
}

#[test]
fn line_and_normalized_utf8_data_ceilings_accept_exact_size_and_reject_excess() {
    let exact_line = format!(":{}\n", "x".repeat(MAX_LINE_BYTES - 1));
    assert!(
        feed([exact_line.into_bytes()])
            .expect("exact comment line")
            .is_empty()
    );
    for input in [
        format!(":{}\n", "x".repeat(MAX_LINE_BYTES)),
        "x".repeat(MAX_LINE_BYTES + 1),
    ] {
        let error = feed([input.into_bytes()]).expect_err("oversized line");
        assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);
    }
    let half = "x".repeat(MAX_DATA_BYTES / 2);
    let exact_data = format!("data: {half}\ndata: {}\n\n", &half[..half.len() - 1]);
    let events = feed([exact_data.into_bytes()]).expect("exact returned-data bound");
    assert_eq!(events[0].data().len(), MAX_DATA_BYTES);
    let excess_data = format!("data: {half}\ndata: {half}\n\n");
    let error = feed([excess_data.into_bytes()]).expect_err("first excess data byte");
    assert_eq!(error.kind(), RecordErrorKind::LimitExceeded);
    let mut invalid = b"data: ".to_vec();
    invalid.extend(std::iter::repeat_n(0xff, MAX_DATA_BYTES / 3 + 1));
    invalid.extend_from_slice(b"\n\n");
    assert_eq!(
        feed([invalid])
            .expect_err("replacement bytes count toward the limit")
            .kind(),
        RecordErrorKind::LimitExceeded
    );
}

#[test]
fn stop_response_ignores_all_bytes_and_media_contract_is_exact() {
    let mut decoder = SseDecoder::new(true);
    assert!(decoder.stopped);
    let mut buffer = BytesMut::from(b"data: ignored\n\n".as_slice());
    assert!(decoder.decode(&mut buffer).expect("204").is_none());
    assert!(buffer.is_empty());
    for media in [
        "text/event-stream",
        "TEXT/EVENT-STREAM",
        "text/event-stream; charset=utf-8",
        "text/event-stream; charset=windows-1252",
    ] {
        assert!(is_event_stream_media_type(media), "valid media {media:?}");
    }
    for media in [
        "application/json",
        "text/event-streaming",
        "application/event-stream",
        "text/event-stream; broken=\"unterminated",
    ] {
        assert!(
            !is_event_stream_media_type(media),
            "invalid media {media:?}"
        );
    }
    fn buffered<R: ResponseRepresentation<Vec<ServerEvent>, Driver = Buffered>>() {}
    fn streaming<R: ResponseRepresentation<ItemStream<ServerEvent>, Driver = Streaming>>() {}
    buffered::<EventStream>();
    streaming::<EventStream>();
    assert_eq!(
        <EventStream as ResponseRepresentation<Vec<ServerEvent>>>::ACCEPT,
        Some(MEDIA_TYPE)
    );
    assert_eq!(SseDecoder::max_buffered_bytes().get(), MAX_FRAME_BYTES);
}

#[test]
fn diagnostic_formats_and_sources_do_not_contain_event_values() {
    let event = event(
        "private-data-sentinel",
        "private-type-sentinel",
        "private-id-sentinel",
        Some(123456),
    );
    let diagnostic = format!("{event:?}");
    for sentinel in [
        "private-data-sentinel",
        "private-type-sentinel",
        "private-id-sentinel",
        "123456",
    ] {
        assert!(!diagnostic.contains(sentinel));
    }
    let input = format!(
        "data: private-limit-sentinel{}\n\n",
        "x".repeat(MAX_LINE_BYTES)
    );
    let error = feed([input.into_bytes()]).expect_err("line limit");
    assert!(!format!("{error} {error:?}").contains("private-limit-sentinel"));
    assert!(error.source().is_none());
    assert_eq!(event.last_event_id(), event.id());
    assert_eq!(event.event_type(), "private-type-sentinel");
    assert_eq!(event.event(), event.event_type());
    assert_eq!(event.retry(), Some(Duration::from_millis(123456)));
}
