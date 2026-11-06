#[test]
fn json_uses_core_contract() {
    use concord_core::{Decode, Encode, bytes::Bytes};
    use concord_codecs::Json;
    assert_eq!(Json::encode(7_u32).unwrap().as_bytes(), Some(&b"7"[..]));
    assert_eq!(<Json as Decode<u32>>::decode_body((), Bytes::from_static(b"7")).unwrap(), 7);
}
