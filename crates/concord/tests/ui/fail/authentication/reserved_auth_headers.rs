macro_rules! reserved_auth_header_case {
    ($module:ident, $header:literal) => {
        mod $module {
            use concord::api;

            api! {
                client ReservedAuthHeaderApi {
                    timeout off;
                    base "https://example.test/";
                    credentials {
                        secret key;
                        credential key = api_key(secret.key);
                    }
                    auth { header $header = key; }


                GET read() -> () using NoContent;
            }}
        }
    };
}

reserved_auth_header_case!(host, "Host");
reserved_auth_header_case!(content_length, "Content-Length");
reserved_auth_header_case!(transfer_encoding, "Transfer-Encoding");
reserved_auth_header_case!(connection, "Connection");
reserved_auth_header_case!(keep_alive, "Keep-Alive");
reserved_auth_header_case!(te, "TE");
reserved_auth_header_case!(trailer, "Trailer");
reserved_auth_header_case!(upgrade, "Upgrade");
reserved_auth_header_case!(proxy_authorization, "Proxy-Authorization");
reserved_auth_header_case!(authorization, "Authorization");
reserved_auth_header_case!(cookie, "Cookie");
reserved_auth_header_case!(set_cookie, "Set-Cookie");
reserved_auth_header_case!(accept, "Accept");
reserved_auth_header_case!(content_type, "Content-Type");
reserved_auth_header_case!(user_agent, "User-Agent");

fn main() {}
