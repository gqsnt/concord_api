use concord_macros::api;

api! {
    client FtpBaseApi {
        base "ftp://example.com"
    }
}

api! {
    client EmptyHostBaseApi {
        base "https://"
    }
}

fn main() {}
