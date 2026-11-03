#[cfg(test)]
mod tests {
    use concord_core::{
        Decode, Encode, ResponseHead,
        bytes::Bytes,
        reqwest::{
            StatusCode, Version,
            header::{HeaderMap, HeaderValue},
        },
    };
    use concord_extras::{Binary, Form, IgnoreContentType, Json, Text};
    use std::collections::BTreeMap;

    fn headers(media: &'static str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("content-type", HeaderValue::from_static(media));
        h
    }
    #[test]
    fn json_and_adapter() {
        let _decoder = concord_core::Decoder::<u32>::new::<Json>();
        let h = headers("application/json");
        let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &h);
        <Json as Decode<u32>>::check_content_type(&head).unwrap();
        let context = <Json as Decode<u32>>::context(&head).unwrap();
        assert_eq!(
            <Json as Decode<u32>>::decode_body(context, Bytes::from_static(b"7")).unwrap(),
            7
        );
        assert_eq!(Json::encode(7u32).unwrap().as_bytes(), Some(&b"7"[..]));
        let h = headers("text/html");
        let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &h);
        assert!(<Json as Decode<u32>>::check_content_type(&head).is_err());
        <IgnoreContentType<Json> as Decode<u32>>::check_content_type(&head).unwrap();
        let context = <IgnoreContentType<Json> as Decode<u32>>::context(&head).unwrap();
        assert_eq!(
            <IgnoreContentType<Json> as Decode<u32>>::decode_body(
                context,
                Bytes::from_static(b"8")
            )
            .unwrap(),
            8
        );
        assert!(
            <IgnoreContentType<Json> as Decode<u32>>::decode_body((), Bytes::from_static(b"bad"))
                .is_err()
        );
    }
    #[test]
    fn text_and_binary() {
        let h = headers("text/plain; charset=iso-8859-1");
        let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &h);
        assert!(<Text as Decode<String>>::check_content_type(&head).is_err());
        <Binary as Decode<Vec<u8>>>::check_content_type(&head).unwrap();
        assert_eq!(
            <Binary as Decode<Vec<u8>>>::decode_body((), Bytes::from_static(b"\xff")).unwrap(),
            [255]
        );
        assert_eq!(
            Binary::encode(vec![255u8]).unwrap().as_bytes(),
            Some(&[255][..])
        );
        let h = headers("text/plain; charset=utf-8");
        let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &h);
        <Text as Decode<String>>::check_content_type(&head).unwrap();
        assert_eq!(
            Text::decode_body((), Bytes::from_static(b"hello")).unwrap(),
            "hello"
        );
        assert_eq!(
            Text::encode("hello".to_owned()).unwrap().as_bytes(),
            Some(&b"hello"[..])
        );
        assert!(Text::decode_body((), Bytes::from_static(b"\xff")).is_err());
    }
    #[test]
    fn form_roundtrip_and_head_check() {
        let h = headers("application/x-www-form-urlencoded");
        let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &h);
        <Form as Decode<BTreeMap<String, String>>>::check_content_type(&head).unwrap();
        let value = BTreeMap::from([("name".to_owned(), "a b".to_owned())]);
        let body = Form::encode(value.clone()).unwrap();
        assert_eq!(body.as_bytes(), Some(&b"name=a+b"[..]));
        let decoded: BTreeMap<String, String> =
            Form::decode_body((), Bytes::copy_from_slice(body.as_bytes().unwrap())).unwrap();
        assert_eq!(decoded, value);
        let h = headers("application/json");
        assert!(
            <Form as Decode<BTreeMap<String, String>>>::check_content_type(&ResponseHead::new(
                StatusCode::OK,
                Version::HTTP_11,
                &h
            ))
            .is_err()
        );
    }
}
