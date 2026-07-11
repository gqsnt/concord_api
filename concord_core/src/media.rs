use crate::codec::ContentType;
use std::fmt;

macro_rules! content_marker {
    ($name:ident, $content_type:literal) => {
        #[derive(Clone, Copy, Default, Eq, PartialEq)]
        pub struct $name;

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(stringify!($name))
            }
        }

        impl ContentType for $name {
            const CONTENT_TYPE: &'static str = $content_type;
        }
    };
}

content_marker!(JsonContentType, "application/json");
content_marker!(TextContentType, "text/plain; charset=utf-8");
content_marker!(OctetStream, "application/octet-stream");
content_marker!(Mp3, "audio/mpeg");
content_marker!(Mp4, "video/mp4");
content_marker!(Pdf, "application/pdf");
content_marker!(Zip, "application/zip");
content_marker!(Png, "image/png");
content_marker!(Jpeg, "image/jpeg");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_content_types_have_expected_content_types() {
        assert_eq!(
            JsonContentType::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("application/json")
        );
        assert_eq!(
            TextContentType::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("text/plain; charset=utf-8")
        );
        assert_eq!(
            OctetStream::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("application/octet-stream")
        );
        assert_eq!(
            crate::multipart::FormData::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("multipart/form-data")
        );
        assert_eq!(JsonContentType::CONTENT_TYPE, "application/json");
        assert_eq!(TextContentType::CONTENT_TYPE, "text/plain; charset=utf-8");
        assert_eq!(OctetStream::CONTENT_TYPE, "application/octet-stream");
        assert_eq!(Mp3::CONTENT_TYPE, "audio/mpeg");
        assert_eq!(Mp4::CONTENT_TYPE, "video/mp4");
        assert_eq!(Pdf::CONTENT_TYPE, "application/pdf");
        assert_eq!(Zip::CONTENT_TYPE, "application/zip");
        assert_eq!(Png::CONTENT_TYPE, "image/png");
        assert_eq!(Jpeg::CONTENT_TYPE, "image/jpeg");
        assert_eq!(format!("{:?}", OctetStream), "OctetStream");
    }

    #[test]
    fn content_type_header_value_matches_static_constant() {
        assert_eq!(
            JsonContentType::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("application/json")
        );
        assert_eq!(
            TextContentType::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("text/plain; charset=utf-8")
        );
        assert_eq!(
            OctetStream::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("application/octet-stream")
        );
        assert_eq!(
            crate::multipart::FormData::try_header_value().expect("valid content type"),
            http::HeaderValue::from_static("multipart/form-data")
        );
    }
}
