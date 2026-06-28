use std::fmt;

pub trait MediaType: Send + Sync + 'static {
    const CONTENT_TYPE: &'static str;
}

macro_rules! media_marker {
    ($name:ident, $content_type:literal) => {
        #[derive(Clone, Copy, Default, Eq, PartialEq)]
        pub struct $name;

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(stringify!($name))
            }
        }

        impl MediaType for $name {
            const CONTENT_TYPE: &'static str = $content_type;
        }
    };
}

media_marker!(OctetStream, "application/octet-stream");
media_marker!(Mp3, "audio/mpeg");
media_marker!(Mp4, "video/mp4");
media_marker!(Pdf, "application/pdf");
media_marker!(Zip, "application/zip");
media_marker!(Png, "image/png");
media_marker!(Jpeg, "image/jpeg");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_media_types_have_expected_content_types() {
        assert_eq!(OctetStream::CONTENT_TYPE, "application/octet-stream");
        assert_eq!(Mp3::CONTENT_TYPE, "audio/mpeg");
        assert_eq!(Mp4::CONTENT_TYPE, "video/mp4");
        assert_eq!(Pdf::CONTENT_TYPE, "application/pdf");
        assert_eq!(Zip::CONTENT_TYPE, "application/zip");
        assert_eq!(Png::CONTENT_TYPE, "image/png");
        assert_eq!(Jpeg::CONTENT_TYPE, "image/jpeg");
        assert_eq!(format!("{:?}", OctetStream), "OctetStream");
    }
}
