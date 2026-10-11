//! Declaration, schema and baseline-fixture tests for labelled rate keys.

use std::num::NonZeroU32;
use std::time::Duration;

use url::Url;

use super::*;
use crate::error::RateLimitErrorKind;

const CATALOG_RECORDING: &str =
    include_str!("../../../../fixtures/migration/v2_1_catalog_recording.json");
const RIOT_RECORDING: &str =
    include_str!("../../../../fixtures/migration/v2_1_riot_recording.json");

/// Minimal JSON reader for the captured migration fixtures (test-only).
#[derive(Debug)]
enum Json {
    Null,
    Bool,
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    fn parse(text: &str) -> Self {
        let mut parser = JsonParser {
            bytes: text.as_bytes(),
            position: 0,
        };
        let value = parser.value();
        parser.whitespace();
        assert_eq!(parser.position, parser.bytes.len(), "trailing JSON");
        value
    }

    fn get(&self, name: &str) -> &Self {
        match self {
            Self::Object(fields) => fields
                .iter()
                .find(|(field, _)| field == name)
                .map(|(_, value)| value)
                .unwrap_or_else(|| panic!("missing JSON field {name}")),
            other => panic!("not an object: {other:?}"),
        }
    }

    fn array(&self) -> &[Self] {
        match self {
            Self::Array(values) => values,
            other => panic!("not an array: {other:?}"),
        }
    }

    fn string(&self) -> &str {
        match self {
            Self::String(value) => value,
            other => panic!("not a string: {other:?}"),
        }
    }

    fn integer(&self) -> u64 {
        match self {
            Self::Number(value) => value.parse().expect("integer"),
            other => panic!("not a number: {other:?}"),
        }
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl JsonParser<'_> {
    fn whitespace(&mut self) {
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn expect(&mut self, byte: u8) {
        self.whitespace();
        assert_eq!(self.bytes[self.position], byte, "JSON syntax");
        self.position += 1;
    }

    fn peek(&mut self) -> u8 {
        self.whitespace();
        self.bytes[self.position]
    }

    fn value(&mut self) -> Json {
        match self.peek() {
            b'{' => {
                self.expect(b'{');
                let mut fields = Vec::new();
                if self.peek() == b'}' {
                    self.expect(b'}');
                    return Json::Object(fields);
                }
                loop {
                    let name = self.string();
                    self.expect(b':');
                    fields.push((name, self.value()));
                    if self.peek() == b',' {
                        self.expect(b',');
                    } else {
                        self.expect(b'}');
                        return Json::Object(fields);
                    }
                }
            }
            b'[' => {
                self.expect(b'[');
                let mut values = Vec::new();
                if self.peek() == b']' {
                    self.expect(b']');
                    return Json::Array(values);
                }
                loop {
                    values.push(self.value());
                    if self.peek() == b',' {
                        self.expect(b',');
                    } else {
                        self.expect(b']');
                        return Json::Array(values);
                    }
                }
            }
            b'"' => Json::String(self.string()),
            b't' => self.literal("true", Json::Bool),
            b'f' => self.literal("false", Json::Bool),
            b'n' => self.literal("null", Json::Null),
            _ => {
                let start = self.position;
                while self
                    .bytes
                    .get(self.position)
                    .is_some_and(|byte| b"-+.eE0123456789".contains(byte))
                {
                    self.position += 1;
                }
                Json::Number(
                    std::str::from_utf8(&self.bytes[start..self.position])
                        .expect("number")
                        .to_owned(),
                )
            }
        }
    }

    fn literal(&mut self, text: &str, value: Json) -> Json {
        assert!(self.bytes[self.position..].starts_with(text.as_bytes()));
        self.position += text.len();
        value
    }

    fn string(&mut self) -> String {
        self.expect(b'"');
        let mut output = Vec::new();
        loop {
            let byte = self.bytes[self.position];
            self.position += 1;
            match byte {
                b'"' => return String::from_utf8(output).expect("UTF-8 JSON string"),
                b'\\' => {
                    let escape = self.bytes[self.position];
                    self.position += 1;
                    match escape {
                        b'"' => output.push(b'"'),
                        b'\\' => output.push(b'\\'),
                        b'/' => output.push(b'/'),
                        b'n' => output.push(b'\n'),
                        b't' => output.push(b'\t'),
                        b'r' => output.push(b'\r'),
                        b'u' => {
                            let digits =
                                std::str::from_utf8(&self.bytes[self.position..self.position + 4])
                                    .expect("escape digits");
                            self.position += 4;
                            let scalar = char::from_u32(
                                u32::from_str_radix(digits, 16).expect("hex escape"),
                            )
                            .expect("scalar escape");
                            let mut buffer = [0; 4];
                            output.extend_from_slice(scalar.encode_utf8(&mut buffer).as_bytes());
                        }
                        other => panic!("unsupported JSON escape {other}"),
                    }
                }
                other => output.push(other),
            }
        }
    }
}

fn unhex(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    (0..text.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&text[index..index + 2], 16).expect("hex byte"))
        .collect()
}

/// Splits one baseline canonical record into its tag and payload.
fn split_record(bytes: &[u8]) -> (&str, &[u8]) {
    let tag_length = usize::from(bytes[0]);
    let tag = std::str::from_utf8(&bytes[1..=tag_length]).expect("ASCII tag");
    let length_start = 1 + tag_length;
    let length = u64::from_be_bytes(
        bytes[length_start..length_start + 8]
            .try_into()
            .expect("u64 length"),
    );
    let payload = &bytes[length_start + 8..];
    assert_eq!(payload.len() as u64, length, "single framed record");
    (tag, payload)
}

fn recordings() -> Vec<(&'static str, Json)> {
    vec![
        ("catalog", Json::parse(CATALOG_RECORDING)),
        ("riot", Json::parse(RIOT_RECORDING)),
    ]
}

fn logical_origin_of(record: &Json) -> String {
    let observed = Url::parse(record.get("observed_url").string()).expect("observed URL");
    normalized_origin(&observed).expect("fixture origin")
}

#[test]
fn v2_1_fixture_key_parts_reencode_to_identical_bytes() {
    let mut checked = 0_usize;
    for (name, recording) in recordings() {
        for record in recording.array() {
            let limiter = record.get("limiter");
            let endpoint = record.get("endpoint_id").string();
            let recorded_origin = limiter.get("logical_origin").string();
            assert_eq!(
                logical_origin_of(record),
                recorded_origin,
                "{name}: v3 origin normalization equals the recorded logical origin"
            );
            for policy in limiter.get("policies").array() {
                for part in policy.get("key").array() {
                    let label = part.array()[0].string();
                    let bytes = unhex(part.array()[1].string());
                    let (tag, payload) = split_record(&bytes);
                    assert_eq!(tag, "utf8", "{name}: every recorded dimension is utf8");
                    let text = std::str::from_utf8(payload).expect("utf8 payload");
                    let reencoded =
                        encode_canonical(text, MAX_RATE_LIMIT_KEY_BYTES).expect("re-encode");
                    assert_eq!(reencoded.as_bytes(), bytes.as_slice(), "{name}/{label}");
                    assert_eq!(reencoded.encoding_id(), UTF8_ENCODING_ID);
                    match label {
                        "origin" => assert_eq!(text, recorded_origin),
                        "endpoint" => assert_eq!(text, endpoint),
                        _ => {}
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked >= 12 + 92, "every captured key part was checked");
}

fn declaration_for(policy: &Json) -> RateLimitDeclaration {
    let mut declaration = RateLimitDeclaration::new(policy.get("id").string());
    let cost = u32::try_from(policy.get("cost").integer()).expect("cost");
    if cost != 1 {
        declaration = declaration.cost(cost);
    }
    for window in policy.get("windows").array() {
        let window = window.array();
        declaration = declaration.window(
            u32::try_from(window[0].integer()).expect("capacity"),
            Duration::from_millis(window[1].integer()),
        );
    }
    for part in policy.get("key").array() {
        let label = part.array()[0].string();
        declaration = match label {
            "origin" => declaration.origin(),
            "endpoint" => declaration.endpoint(),
            custom => {
                let bytes = unhex(part.array()[1].string());
                let (_, payload) = split_record(&bytes);
                let text = String::from_utf8(payload.to_vec()).expect("utf8 payload");
                declaration.part(custom, &text)
            }
        };
    }
    declaration
}

#[test]
fn v2_1_fixture_policies_resolve_from_labelled_declarations_byte_for_byte() {
    let mut policies = 0_usize;
    for (name, recording) in recordings() {
        for record in recording.array() {
            let origin = logical_origin_of(record);
            let endpoint = record.get("endpoint_id").string();
            let recorded = record.get("limiter").get("policies").array();
            let mut resolved = Vec::new();
            for policy in recorded {
                let declaration = declaration_for(policy);
                declaration
                    .validate()
                    .expect("fixture declaration is valid v3");
                let resolved_policy = declaration.resolve(&origin, endpoint).expect("resolve");
                RateLimitPlan::push_policy(&mut resolved, resolved_policy).expect("plan");
            }
            let plan = RateLimitPlan::from_validated_policies(resolved);
            assert_eq!(plan.policies().len(), recorded.len(), "{name}");
            assert_eq!(
                plan.uses_origin(),
                recorded.iter().any(|policy| {
                    policy
                        .get("key")
                        .array()
                        .iter()
                        .any(|part| part.array()[0].string() == "origin")
                })
            );
            for (policy, expected) in plan.policies().iter().zip(recorded) {
                assert_eq!(policy.id().as_str(), expected.get("id").string());
                assert_eq!(
                    u64::from(policy.cost().get()),
                    expected.get("cost").integer()
                );
                let windows = policy
                    .windows()
                    .iter()
                    .map(|window| {
                        (
                            u64::from(window.capacity().get()),
                            u64::try_from(window.duration().as_millis()).expect("ms"),
                        )
                    })
                    .collect::<Vec<_>>();
                let expected_windows = expected
                    .get("windows")
                    .array()
                    .iter()
                    .map(|window| (window.array()[0].integer(), window.array()[1].integer()))
                    .collect::<Vec<_>>();
                assert_eq!(windows, expected_windows, "{name}: source window order");
                let parts = policy.key().parts();
                let expected_parts = expected.get("key").array();
                assert_eq!(parts.len(), expected_parts.len());
                for (part, expected_part) in parts.iter().zip(expected_parts) {
                    assert_eq!(part.name(), expected_part.array()[0].string());
                    assert_eq!(
                        part.value(),
                        unhex(expected_part.array()[1].string()).as_slice(),
                        "{name}: {} bytes",
                        part.name()
                    );
                    assert_eq!(part.encoding_id(), UTF8_ENCODING_ID);
                }
                policies += 1;
            }
        }
    }
    assert!(policies >= 12 + 92);
}

#[test]
fn catalog_client_tenant_and_tenant_labels_share_bytes_but_not_schema_or_bucket() {
    let recording = Json::parse(CATALOG_RECORDING);
    let update = recording
        .array()
        .iter()
        .find(|record| record.get("endpoint_id").string() == "catalog.item.update")
        .expect("update recording");
    let policies = update.get("limiter").get("policies").array();
    let read_tenant = &policies[0].get("key").array()[1];
    let write_tenant = &policies[1].get("key").array()[2];
    assert_eq!(read_tenant.array()[0].string(), "tenant");
    assert_eq!(write_tenant.array()[0].string(), "client.tenant");
    assert_eq!(
        read_tenant.array()[1].string(),
        write_tenant.array()[1].string(),
        "the baseline value bytes are identical"
    );

    let origin = logical_origin_of(update);
    let resolve = |label: &str| {
        RateLimitDeclaration::new("catalog.tenant.read")
            .window(120, Duration::from_secs(60))
            .window(5_000, Duration::from_secs(3_600))
            .origin()
            .part(label, "tenant-a")
            .resolve(&origin, "catalog.item.update")
            .expect("resolved")
    };
    let tenant = resolve("tenant");
    let client_tenant = resolve("client.tenant");
    assert_eq!(
        tenant.key().parts()[1].value(),
        unhex(read_tenant.array()[1].string()).as_slice(),
        "the selected preserved key equals the baseline bytes"
    );
    assert_eq!(
        tenant.key().parts()[1].value(),
        client_tenant.key().parts()[1].value()
    );
    assert_ne!(tenant.schema(), client_tenant.schema());
    let error = RateLimitPlan::try_from_policies([tenant, client_tenant])
        .err()
        .expect("same ID with a different label is a conflicting identity");
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidConfiguration);
}

#[test]
fn origin_normalization_is_complete_explicit_and_lowercase() {
    for (url, expected) in [
        (
            "HTTPS://API.Example.TEST/v1/items?x=1",
            "https://api.example.test:443",
        ),
        ("http://example.test/", "http://example.test:80"),
        ("https://example.test:443/", "https://example.test:443"),
        ("https://example.test:8443/", "https://example.test:8443"),
        ("http://127.0.0.1:8080/", "http://127.0.0.1:8080"),
        ("https://[2001:DB8::1]/", "https://[2001:db8::1]:443"),
        ("http://[::1]:9/", "http://[::1]:9"),
    ] {
        assert_eq!(
            normalized_origin(&Url::parse(url).expect("URL")).as_deref(),
            Some(expected),
            "{url}"
        );
    }
}

const SCHEMA_VALUE_SENTINEL: &str = "schema-key-value-sentinel-4c1e";

fn policy_with(labels: &[&str], windows: &[(u32, u64)], cost: u32) -> RateLimitPolicy {
    let mut declaration = RateLimitDeclaration::new("schema.policy").cost(cost);
    for (capacity, seconds) in windows {
        declaration = declaration.window(*capacity, Duration::from_secs(*seconds));
    }
    for label in labels {
        declaration = match *label {
            "origin" => declaration.origin(),
            "endpoint" => declaration.endpoint(),
            custom => declaration.part(custom, SCHEMA_VALUE_SENTINEL),
        };
    }
    declaration
        .resolve("https://schema.example.test:443", "schema.endpoint")
        .expect("schema policy")
}

#[test]
fn schema_is_value_free_ordered_and_canonical_in_windows_but_excludes_cost() {
    let policy = policy_with(&["origin", "endpoint", "tenant"], &[(100, 60), (10, 1)], 2);
    let schema = policy.schema();
    assert_eq!(schema.id().as_str(), "schema.policy");
    assert_eq!(schema.framing_version(), RATE_KEY_FRAMING_VERSION);
    assert_eq!(RATE_KEY_FRAMING_VERSION, 1);
    let dimensions = schema
        .dimensions()
        .iter()
        .map(|dimension| (dimension.label(), dimension.kind(), dimension.encoding_id()))
        .collect::<Vec<_>>();
    assert_eq!(
        dimensions,
        [
            ("origin", RateDimensionKind::Origin, "concord.utf8/1"),
            ("endpoint", RateDimensionKind::Endpoint, "concord.utf8/1"),
            ("tenant", RateDimensionKind::Custom, "concord.utf8/1"),
        ]
    );
    assert_eq!(
        schema
            .windows()
            .iter()
            .map(|window| (window.duration().as_secs(), window.capacity().get()))
            .collect::<Vec<_>>(),
        [(1, 10), (60, 100)]
    );
    assert!(schema.uses_origin());
    let rendered = format!("{schema:?}");
    assert!(
        !rendered.contains(SCHEMA_VALUE_SENTINEL),
        "no key value in the schema"
    );
    assert!(!rendered.contains("schema.example.test"));

    // Source order is provenance; canonical windows compare equal.
    assert_eq!(
        policy_with(&["origin", "endpoint", "tenant"], &[(10, 1), (100, 60)], 2).schema(),
        schema
    );
    // Cost is not part of the schema (nor of bucket identity).
    assert_eq!(
        policy_with(&["origin", "endpoint", "tenant"], &[(10, 1), (100, 60)], 1).schema(),
        schema
    );
    for different in [
        policy_with(&["origin", "tenant", "endpoint"], &[(10, 1), (100, 60)], 2),
        policy_with(
            &["origin", "endpoint", "client.tenant"],
            &[(10, 1), (100, 60)],
            2,
        ),
        policy_with(&["origin", "endpoint"], &[(10, 1), (100, 60)], 2),
        policy_with(&["origin", "endpoint", "tenant"], &[(10, 1), (100, 61)], 2),
        policy_with(&["origin", "endpoint", "tenant"], &[(11, 1), (100, 60)], 2),
    ] {
        assert_ne!(different.schema(), schema);
    }
    let custom_encoding = RateLimitDeclaration::new("schema.policy")
        .cost(2)
        .window(10, Duration::from_secs(1))
        .window(100, Duration::from_secs(60))
        .origin()
        .endpoint()
        .part("tenant", &7_u64)
        .resolve("https://schema.example.test:443", "schema.endpoint")
        .expect("u64 tenant");
    assert_ne!(custom_encoding.schema(), schema);
}

#[test]
fn declarations_reject_incomplete_or_invalid_structure_with_exact_kinds() {
    let valid = || {
        RateLimitDeclaration::new("valid")
            .window(2, Duration::from_secs(1))
            .origin()
    };
    valid().validate().expect("valid declaration");
    assert_eq!(valid().id().map(RateLimitId::as_str), Some("valid"));
    for (declaration, kind) in [
        (
            RateLimitDeclaration::new("no.windows").origin(),
            RateLimitErrorKind::InvalidConfiguration,
        ),
        (
            RateLimitDeclaration::new("no.key").window(1, Duration::from_secs(1)),
            RateLimitErrorKind::InvalidKey,
        ),
        (valid().cost(3), RateLimitErrorKind::InvalidConfiguration),
        (valid().cost(0), RateLimitErrorKind::InvalidConfiguration),
        (
            valid().cost(1).cost(2),
            RateLimitErrorKind::InvalidConfiguration,
        ),
        (
            valid().window(3, Duration::from_secs(1)),
            RateLimitErrorKind::InvalidConfiguration,
        ),
        (valid().origin(), RateLimitErrorKind::InvalidKey),
        (
            valid().part("endpoint", "x"),
            RateLimitErrorKind::InvalidKey,
        ),
        (valid().part("", "x"), RateLimitErrorKind::InvalidKey),
        (
            RateLimitDeclaration::new(""),
            RateLimitErrorKind::InvalidConfiguration,
        ),
    ] {
        assert_eq!(
            declaration
                .validate()
                .expect_err("invalid declaration")
                .kind(),
            kind
        );
        assert!(declaration.resolve("https://x.test:443", "x.y").is_err());
    }
    let mut maximum = RateLimitDeclaration::new("maximum").window(1, Duration::from_secs(1));
    for index in 0..MAX_RATE_LIMIT_KEY_PARTS - 2 {
        maximum = maximum.part(&format!("part-{index}"), &index);
    }
    let maximum = maximum.origin().endpoint();
    let policy = maximum
        .resolve("https://x.test:443", "x.y")
        .expect("maximum dimensions");
    assert_eq!(policy.key().parts().len(), MAX_RATE_LIMIT_KEY_PARTS);
    assert_eq!(
        maximum
            .part("excess", "x")
            .validate()
            .expect_err("first excess dimension")
            .kind(),
        RateLimitErrorKind::InvalidKey
    );
}

#[test]
fn resolved_built_in_dimensions_are_bounded_by_the_aggregate_key_limit() {
    let label = "c".repeat(MAX_RATE_LIMIT_KEY_PART_NAME_BYTES);
    let filler = "f".repeat(MAX_RATE_LIMIT_KEY_BYTES - 12 - label.len() - 13 - 40);
    let declaration = RateLimitDeclaration::new("bounded")
        .window(1, Duration::from_secs(1))
        .part(&label, &filler)
        .origin();
    // 40 spare bytes cannot hold the origin record (12 + 6 + 13 + payload).
    assert_eq!(
        declaration
            .resolve("https://a-long-origin.example.test:443", "x.y")
            .err()
            .expect("origin beyond the aggregate key bound")
            .kind(),
        RateLimitErrorKind::InvalidKey
    );
    declaration
        .resolve("h://a:1", "x.y")
        .expect("a short origin fits the remaining budget");
}

#[test]
fn plan_summary_discloses_schema_and_the_actual_enforcement_choice() {
    let policy = policy_with(&["origin", "tenant"], &[(10, 1)], 3);
    let plan = RateLimitPlan::try_from_policies([policy]).expect("plan");
    let summaries = [
        (
            None,
            None,
            "declared policies; no limiter selected, so admission fails closed",
        ),
        (
            Some(NoopRateLimiter::CAPABILITY),
            Some(Enforcement::Unenforced),
            "declared policies; enforcement explicitly disabled by application selection",
        ),
        (
            Some(LimiterCapability::new(Enforcement::Local, "test.local", 1)),
            Some(Enforcement::Local),
            "declared policies; enforced by the application-selected process-local limiter",
        ),
        (
            Some(LimiterCapability::new(
                Enforcement::SharedPersistent,
                "test.shared",
                7,
            )),
            Some(Enforcement::SharedPersistent),
            "declared policies; enforced by the application-selected shared persistent limiter",
        ),
    ];
    for (limiter, enforcement, label) in summaries {
        let summary = RatePlanSummary::new(&plan, limiter);
        assert_eq!(summary.policy_count(), 1);
        assert_eq!(summary.limiter(), limiter);
        assert_eq!(summary.enforcement(), enforcement);
        assert_eq!(summary.enforcement_label(), label);
        let policy = &summary.policies()[0];
        assert_eq!(policy.cost(), NonZeroU32::new(3).expect("cost"));
        assert_eq!(policy.schema().dimensions()[1].label(), "tenant");
        let rendered = format!("{summary:?}");
        assert!(!rendered.contains(SCHEMA_VALUE_SENTINEL));
        assert!(!rendered.contains("schema.example.test"));
    }
    let empty = RatePlanSummary::new(&RateLimitPlan::empty(), Some(NoopRateLimiter::CAPABILITY));
    assert_eq!(empty.enforcement(), None);
    assert_eq!(empty.enforcement_label(), "no declared rate-limit policies");
}
