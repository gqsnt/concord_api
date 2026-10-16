//! Exact canonical-byte contracts for built-in and custom V3 rate keys.

use super::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn encode<T: RateKeyPart + ?Sized>(value: &T) -> EncodedKeyPart {
    encode_canonical(value, MAX_RATE_LIMIT_KEY_BYTES).expect("canonical value")
}

fn assert_encoding<T: RateKeyPart + ?Sized>(value: &T, encoding_id: &str, expected_hex: &str) {
    let encoded = encode(value);
    assert_eq!(encoded.encoding_id(), encoding_id);
    assert_eq!(T::ENCODING_ID, encoding_id);
    assert_eq!(hex(encoded.as_bytes()), expected_hex);
}

/// Independent expression of the canonical framing contract:
/// `u8(tag.len) || tag || u64_be(value.len) || value`.
fn canonical_record(type_tag: &str, value: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[type_tag.len() as u8]);
    bytes.extend_from_slice(type_tag.as_bytes());
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.extend_from_slice(value);
    bytes
}

/// Label "tenant", value "tenant-a" from `v3_catalog_recording.json`.
const FIXTURE_TENANT_A: &str = "0475746638000000000000000874656e616e742d61";
/// The `origin` part of every `eu` Catalog recording.
const FIXTURE_EU_ORIGIN: &str = "0475746638000000000000002768747470733a2f2f65752e6170692e636174616c6f672e6578616d706c652e746573743a343433";
/// The `endpoint` part of the Catalog `update_item` recording.
const FIXTURE_UPDATE_ENDPOINT: &str =
    "04757466380000000000000013636174616c6f672e6974656d2e757064617465";
/// The `origin` part of the Riot `na1` recordings.
const FIXTURE_NA1_ORIGIN: &str =
    "0475746638000000000000002168747470733a2f2f6e61312e6170692e72696f7467616d65732e636f6d3a343433";

#[test]
fn utf8_strings_match_the_captured_contract_bytes() {
    assert_encoding("tenant-a", "concord.utf8/1", FIXTURE_TENANT_A);
    assert_encoding(
        &String::from("tenant-a"),
        "concord.utf8/1",
        FIXTURE_TENANT_A,
    );
    assert_encoding(
        "https://eu.api.catalog.example.test:443",
        "concord.utf8/1",
        FIXTURE_EU_ORIGIN,
    );
    assert_encoding(
        "catalog.item.update",
        "concord.utf8/1",
        FIXTURE_UPDATE_ENDPOINT,
    );
    assert_encoding(
        "https://na1.api.riotgames.com:443",
        "concord.utf8/1",
        FIXTURE_NA1_ORIGIN,
    );
    assert_encoding("", "concord.utf8/1", "04757466380000000000000000");
    assert_encoding("é", "concord.utf8/1", "04757466380000000000000002c3a9");
    assert_eq!(UTF8_ENCODING_ID, "concord.utf8/1");
}

#[test]
fn bool_and_char_match_the_canonical_tags_and_payloads() {
    assert_encoding(&true, "concord.bool/1", "04626f6f6c000000000000000101");
    assert_encoding(&false, "concord.bool/1", "04626f6f6c000000000000000100");
    assert_encoding(&'A', "concord.char/1", "0463686172000000000000000400000041");
    assert_encoding(
        &'\u{20ac}',
        "concord.char/1",
        "04636861720000000000000004000020ac",
    );
    assert_encoding(
        &'\u{1f600}',
        "concord.char/1",
        "046368617200000000000000040001f600",
    );
}

#[test]
fn every_integer_width_matches_the_canonical_big_endian_bytes() {
    assert_encoding(&7_u8, "concord.u8/1", "027538000000000000000107");
    assert_encoding(&0x1234_u16, "concord.u16/1", "0375313600000000000000021234");
    assert_encoding(
        &0x0102_0304_u32,
        "concord.u32/1",
        "03753332000000000000000401020304",
    );
    assert_encoding(
        &u32::MAX,
        "concord.u32/1",
        "037533320000000000000004ffffffff",
    );
    assert_encoding(
        &0x0102_0304_0506_0708_u64,
        "concord.u64/1",
        "0375363400000000000000080102030405060708",
    );
    assert_encoding(
        &0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10_u128,
        "concord.u128/1",
        "047531323800000000000000100102030405060708090a0b0c0d0e0f10",
    );
    assert_encoding(&-1_i8, "concord.i8/1", "0269380000000000000001ff");
    assert_encoding(&-2_i16, "concord.i16/1", "036931360000000000000002fffe");
    assert_encoding(&-3_i32, "concord.i32/1", "036933320000000000000004fffffffd");
    assert_encoding(
        &-4_i64,
        "concord.i64/1",
        "036936340000000000000008fffffffffffffffc",
    );
    assert_encoding(
        &i64::MIN,
        "concord.i64/1",
        "0369363400000000000000088000000000000000",
    );
    assert_encoding(
        &-5_i128,
        "concord.i128/1",
        "04693132380000000000000010fffffffffffffffffffffffffffffffb",
    );
}

#[test]
fn pointer_sized_integers_use_the_canonical_64_bit_conversion() {
    assert_encoding(
        &42_usize,
        "concord.usize/1",
        "057573697a650000000000000008000000000000002a",
    );
    assert_encoding(
        &-6_isize,
        "concord.isize/1",
        "056973697a650000000000000008fffffffffffffffa",
    );
    #[cfg(target_pointer_width = "64")]
    assert_encoding(
        &0x0102_0304_0506_0708_usize,
        "concord.usize/1",
        "057573697a6500000000000000080102030405060708",
    );
}

#[test]
fn independent_oracle_matches_canonical_framing_for_every_builtin() {
    fn check<T: RateKeyPart + ?Sized>(value: &T, tag: &str, payload: &[u8]) {
        assert_eq!(
            encode(value).as_bytes(),
            canonical_record(tag, payload).as_slice()
        );
    }
    for text in ["", "a", "tenant-a", "client.tenant", "\u{1f600}\u{0}"] {
        check(text, "utf8", text.as_bytes());
    }
    for flag in [false, true] {
        check(&flag, "bool", &[u8::from(flag)]);
    }
    for scalar in ['\u{0}', 'z', '\u{10ffff}'] {
        check(&scalar, "char", &u32::from(scalar).to_be_bytes());
    }
    for value in [0_u64, 1, u64::MAX] {
        check(&value, "u64", &value.to_be_bytes());
        check(&(value as u8), "u8", &(value as u8).to_be_bytes());
        check(&(value as u16), "u16", &(value as u16).to_be_bytes());
        check(&(value as u32), "u32", &(value as u32).to_be_bytes());
        check(&u128::from(value), "u128", &u128::from(value).to_be_bytes());
        check(&(value as i8), "i8", &(value as i8).to_be_bytes());
        check(&(value as i16), "i16", &(value as i16).to_be_bytes());
        check(&(value as i32), "i32", &(value as i32).to_be_bytes());
        check(&(value as i64), "i64", &(value as i64).to_be_bytes());
        check(&i128::from(value), "i128", &i128::from(value).to_be_bytes());
        let pointer = value as usize;
        check(&pointer, "usize", &(pointer as u64).to_be_bytes());
        let signed = value as isize;
        check(&signed, "isize", &(signed as i64).to_be_bytes());
    }
}

#[test]
fn references_and_smart_pointers_forward_bytes_and_encoding_id() {
    let direct = encode("tenant-a");
    let owned = String::from("tenant-a");
    let forwarded = [
        encode(&&owned),
        encode(&Box::<str>::from("tenant-a")),
        encode(&Arc::<str>::from("tenant-a")),
        encode(&Cow::<str>::Borrowed("tenant-a")),
        encode(&Cow::<str>::Owned(owned.clone())),
        encode(&Arc::new(owned.clone())),
        encode(&Box::new(7_u32)),
    ];
    for part in &forwarded[..6] {
        assert_eq!(part.encoding_id(), direct.encoding_id());
        assert_eq!(part.as_bytes(), direct.as_bytes());
    }
    assert_eq!(forwarded[6].encoding_id(), "concord.u32/1");
    assert_eq!(<&String as RateKeyPart>::ENCODING_ID, "concord.utf8/1");
    assert_eq!(<Box<u64> as RateKeyPart>::ENCODING_ID, "concord.u64/1");
    assert_eq!(<Arc<bool> as RateKeyPart>::ENCODING_ID, "concord.bool/1");
    assert_eq!(<Cow<'_, str> as RateKeyPart>::ENCODING_ID, "concord.utf8/1");
}

#[test]
fn scalar_type_boundaries_are_distinct() {
    assert_ne!(encode(&1_u8).as_bytes(), encode(&1_u16).as_bytes());
    assert_ne!(encode("1").as_bytes(), encode(&1_u8).as_bytes());
    assert_ne!(encode(&1_usize).as_bytes(), encode(&1_u64).as_bytes());
}

#[test]
fn writer_rejects_the_first_excess_without_partial_success() {
    let encoded = encode("bounded");
    encode_canonical("bounded", encoded.len()).expect("exact boundary");
    let error = encode_canonical("bounded", encoded.len() - 1)
        .err()
        .expect("one byte short");
    assert_eq!(error.kind(), RateLimitErrorKind::InvalidKey);

    let mut writer = CanonicalKeyWriter::new(20).expect("writer");
    writer.write_tagged_bytes("u8", &[1]).expect("first record");
    let retained = writer.bytes.len();
    assert!(writer.write_tagged_bytes("utf8", b"too-long").is_err());
    assert_eq!(
        writer.bytes.len(),
        retained,
        "a rejected record retains none of its bytes"
    );
    for tag in ["", "has space", "caf\u{e9}"] {
        assert!(writer.write_tagged_bytes(tag, b"").is_err());
    }
    let long_tag = "t".repeat(256);
    assert!(
        CanonicalKeyWriter::new(1_024)
            .expect("writer")
            .write_tagged_bytes(&long_tag, b"")
            .is_err()
    );
    assert_eq!(
        CanonicalKeyWriter::new(MAX_RATE_LIMIT_KEY_BYTES + 1)
            .err()
            .expect("limit above the hard cap")
            .kind(),
        RateLimitErrorKind::InvalidConfiguration
    );
}

struct TenantId(u64);

impl RateKeyPart for TenantId {
    const ENCODING_ID: &'static str = "com.example.tenant/1";

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        out.write_tagged_bytes("com.example.tenant", &self.0.to_be_bytes())
    }
}

struct DecimalText;

impl RateKeyEncoder<u64> for DecimalText {
    const ENCODING_ID: &'static str = "com.example.decimal/2";

    fn write_canonical(value: &u64, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        value.to_string().write_canonical(out)
    }
}

struct Invalid<const KIND: u8>;

impl<const KIND: u8> RateKeyPart for Invalid<KIND> {
    const ENCODING_ID: &'static str = match KIND {
        0 => "",
        1 => "has space",
        2 => "caf\u{e9}",
        _ => "x",
    };

    fn write_canonical(&self, out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        panic!(
            "an invalid encoding ID must be rejected before writing ({:?})",
            out.limit
        )
    }
}

struct Silent;

impl RateKeyPart for Silent {
    const ENCODING_ID: &'static str = "com.example.silent/1";

    fn write_canonical(&self, _out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        Ok(())
    }
}

struct Overlong;

impl RateKeyEncoder<str> for Overlong {
    const ENCODING_ID: &'static str = "oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo";

    fn write_canonical(_value: &str, _out: &mut CanonicalKeyWriter) -> Result<(), RateLimitError> {
        panic!("an overlong encoding ID must be rejected before writing")
    }
}

#[test]
fn custom_parts_and_encoders_expose_their_encoding_id_separately_from_bytes() {
    let custom = encode(&TenantId(9));
    assert_eq!(custom.encoding_id(), "com.example.tenant/1");
    assert_eq!(
        custom.as_bytes(),
        canonical_record("com.example.tenant", &9_u64.to_be_bytes()).as_slice(),
        "the ENCODING_ID is schema metadata and is not inserted into value bytes"
    );

    let via = encode_canonical_with::<DecimalText, u64>(&42, 64).expect("via encoder");
    assert_eq!(via.encoding_id(), "com.example.decimal/2");
    assert_eq!(via.as_bytes(), encode("42").as_bytes());
    assert_ne!(via.encoding_id(), encode("42").encoding_id());
    assert_eq!(via.len(), via.as_bytes().len());
    assert!(!via.is_empty());
    assert_eq!(via.clone().into_vec(), via.as_bytes());
}

#[test]
fn invalid_encoding_ids_and_empty_encodings_fail_before_retention() {
    for error in [
        encode_canonical(&Invalid::<0>, 64).err(),
        encode_canonical(&Invalid::<1>, 64).err(),
        encode_canonical(&Invalid::<2>, 64).err(),
        encode_canonical_with::<Overlong, str>("value", 64).err(),
    ] {
        assert_eq!(
            error.expect("invalid encoding ID").kind(),
            RateLimitErrorKind::InvalidConfiguration
        );
    }
    assert_eq!(
        encode_canonical(&Silent, 64)
            .err()
            .expect("empty canonical encoding")
            .kind(),
        RateLimitErrorKind::InvalidKey
    );
    let boundary = "i".repeat(MAX_RATE_KEY_ENCODING_ID_BYTES);
    assert!(is_valid_encoding_id(&boundary));
    assert!(!is_valid_encoding_id(&format!("{boundary}i")));
    assert!(!is_valid_encoding_id(""));
    assert!(!is_valid_encoding_id("tab\tid"));
    assert!(is_valid_encoding_id("concord.utf8/1"));
    assert!(Overlong::ENCODING_ID.len() > MAX_RATE_KEY_ENCODING_ID_BYTES);
}
