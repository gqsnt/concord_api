use std::collections::VecDeque;
use std::io::{self, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig, ServerConnection, StreamOwned};

// Fixed, non-secret test fixture material. These keys authenticate only the
// loopback probe server/client pair and are never used as real credentials.
const ROOT_CERTIFICATE_DER_BASE64: &str = concat!(
    "MIIDJTCCAg2gAwIBAgIUVVfNMrqxxHjkQC1jfCVryF3pplkwDQYJKoZIhvcNAQEL",
    "BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MDcxNjE5MzYzOVoXDTM2MDcx",
    "MzE5MzYzOVowFDESMBAGA1UEAwwJbG9jYWxob3N0MIIBIjANBgkqhkiG9w0BAQEF",
    "AAOCAQ8AMIIBCgKCAQEAp3I9k8hbaMpnNeWUAdGNVTGjv2fbf6ewUkbQQTOyQZdu",
    "DH5LN1MbVLoG1Q0BPVeCdEyvPz6YxdgFJh8Njt4pyxTFYORoshkbiLrGBA/26cF/",
    "YfkZfWvUfJUs4wUO4X9+OIVJ+29sZjNXVCbCcuxtNHDszxGBqO759y1I1sb0P4tP",
    "3jf1UNKCbErUnH3lubB18o4gP1M9lbZwJE8/lXTIHz7cIBvAdt7bJULGB1b6/lTz",
    "ESkCIIaClUlVq/t6ntQI77CRAMnpXWC63wvr9jPg+2t4eKwuZoQFKiaq32YsP6Tb",
    "PeZXrMOQtPoCI4OjeI4Tn+fOxHeqPOiXN7H/fQH61wIDAQABo28wbTAdBgNVHQ4E",
    "FgQUgJBAFdGyNKW4yoakaCp6VckcvfQwHwYDVR0jBBgwFoAUgJBAFdGyNKW4yoak",
    "aCp6VckcvfQwDwYDVR0TAQH/BAUwAwEB/zAaBgNVHREEEzARgglsb2NhbGhvc3SH",
    "BH8AAAEwDQYJKoZIhvcNAQELBQADggEBAJv3yYtO2+FKpayvPaxnnpdl+i08uAqF",
    "5ehwkljnT+ZZvLtTcCTB1rgfGqxZkRzaTkDjak4LdpN5UNWUyZythcLFsbcxTSQP",
    "Hikjrc5Aj3lvtuM9P+F2nFBN6U5aUc0lveGe/hfWWU+QrZBd1dLVx1KFJnaf7EXP",
    "FUCy9ic9rdhPqOreELkkI7+GkLJtvnMLaRb85/wyEazJo9WZsFzAQsVoaBI/y/UG",
    "UaoNlOn1D6xIGrt8wgVLrYkYr76DmGBXj3pd1pefBPoattQAuRxkLsZ80pzSrp5B",
    "1xsVlF/c0Fa5VC2W6VRwB8hEzI+yfPZIu4jwnXwNgpXu/wZJXHj4MuM=",
);
const SERVER_CERTIFICATE_DER_BASE64: &str = concat!(
    "MIIDQjCCAiqgAwIBAgINQ09OQ09SRFNFUlZFUjANBgkqhkiG9w0BAQsFADAUMRIw",
    "EAYDVQQDDAlsb2NhbGhvc3QwHhcNMjYwODMxMTkyMDA0WhcNMzYwMzMxMTkyMDA0",
    "WjAUMRIwEAYDVQQDDAlsb2NhbGhvc3QwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAw",
    "ggEKAoIBAQCrrHl4Jp/Vg6OylGhuHxxiWjEew12hc7AZez361OYT/F2PulnP9mso",
    "5Cf6FJ87rkRaMiVPgsRI4AusUv2XErblLlVh1ZZx/uAOk71BKVwVdXxwem8L6Duh",
    "bNMwswNuHzPU3evJft6jmt71LFyV7uZeilbb+UuxY8Pvkn6P/Ony3+V9rzIi1HAK",
    "R35UX3M9N0s46TvVHEUu64Ud0NnlNw/Y+/CD30zWqmLJXBktbJ42SpwAYWbMbSdL",
    "fJFPjfbB6Ty3NRcvdsa1OXsu7pVKxoTcbzTsQfN8Lsz7Cux2SMd4Qtmq5OFqefzL",
    "jFAzvBB45mw0TAdk/aJz+CccBEnNpOEfAgMBAAGjgZIwgY8wDAYDVR0TAQH/BAIw",
    "ADAOBgNVHQ8BAf8EBAMCBaAwEwYDVR0lBAwwCgYIKwYBBQUHAwEwGgYDVR0RBBMw",
    "EYIJbG9jYWxob3N0hwR/AAABMB0GA1UdDgQWBBQI8Dt6EDBzpm2fQM8vddFneIjo",
    "ojAfBgNVHSMEGDAWgBSAkEAV0bI0pbjKhqRoKnpVyRy99DANBgkqhkiG9w0BAQsF",
    "AAOCAQEAm4sCHfVJxAaXKj7E3NwwCeDkkbpAhbR3DpWbVA6wppCqxEEMrqrcQRG7",
    "cX7x0DvbNgSx3/K1xDIl+nsDtvnaJWE2PrAru0aQIjQMKMvURd104vf0KVoj2nS7",
    "pFHYKB3+/oJjdQdiWmHUWQbBAwVr3Oc6w/FN394kcgOgDmel4RaKgfahKlEb/9ij",
    "hS7NrgDY69l42JvXf2KFV7B50lyFal+/TDmOteU95Yc7KMhHIgjYWrm8KwXf7Abl",
    "21WO8n3yM2xKeZqZUFZ1QORTyMEZYXVYhHLfWDn+zP31h+ChoMRdAm5brTRDQhpg",
    "ThekCm4Ba/hR35bLU9yTweCVuV3acw==",
);
const SERVER_PRIVATE_KEY_DER_BASE64: &str = concat!(
    "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCrrHl4Jp/Vg6Oy",
    "lGhuHxxiWjEew12hc7AZez361OYT/F2PulnP9mso5Cf6FJ87rkRaMiVPgsRI4Aus",
    "Uv2XErblLlVh1ZZx/uAOk71BKVwVdXxwem8L6DuhbNMwswNuHzPU3evJft6jmt71",
    "LFyV7uZeilbb+UuxY8Pvkn6P/Ony3+V9rzIi1HAKR35UX3M9N0s46TvVHEUu64Ud",
    "0NnlNw/Y+/CD30zWqmLJXBktbJ42SpwAYWbMbSdLfJFPjfbB6Ty3NRcvdsa1OXsu",
    "7pVKxoTcbzTsQfN8Lsz7Cux2SMd4Qtmq5OFqefzLjFAzvBB45mw0TAdk/aJz+Ccc",
    "BEnNpOEfAgMBAAECggEAEGGJR6870iZZAOnLWzKpxTLA/CP9M7IzzAKoA967w41x",
    "qgzIlCD50bGPpPvBw/KZgBfH2qHqM9/5DSzwGLHG9PBqYkb8r4z3+Twcs3KMYFnR",
    "EXSFUxgKVJlrP3rdaW0dx4KThKsP+IGHI9LjVMUygzPlHOFZWShhYmNSo0+pTSzr",
    "FVfFJPNj4+pN8tBX1TX3tFf8WUgJRj7esv1IwbK4zYozOEKTgQ1k6SApwLJTTrkI",
    "F6RG20Iud5zcjglXYv3qkEzVpo9UY+FmKaYunsLtTRxb0wNkWPQyrNtynBYs8ueY",
    "afjnraoanW0mQm2YFRuS5ryaEM1LJqYWNjoxTxai1QKBgQDXtg32yyxKo7s77ehd",
    "TaoX1DhpAuFtdEXzKUOwempPGqbb1ManbZ3DIuLXuLTVZTHHaau/2bNp79QZE+lR",
    "ZpHClNEWzciTnn0Qa48rWEhIL/egeW8U7Pgo95Zk8rPNuPXDZirYvlOa83fkK8KQ",
    "0fS5KAu3MAAVQnXuyyBcMMCfuwKBgQDLvNQTgGiEE1XW0BImOfu7/NNYl7cLIm/S",
    "b9rEtnPRK0QKVBn8em5nQjc6qobIWBK3yEosAz36R+R5/1japKsHAqsm8tgZx/YT",
    "PNweHRYR7xCv7Cw9nyLXogUZmLbZfE5IFgudhE59NZG5B9ZajgxJpIQ8avjDiiss",
    "ScAwSbBz7QKBgQCqYn0JMVac5lNgdP0TpeLyy4h7OfFB+ZO/DBsRbMBIF28xORJo",
    "8Wap2gaACr+bx0DYmJk3Awh6ZxuLa3Th/TipD89QSd9i6s8yhyMhQ6xkiK76LLEX",
    "QE6MXXSH7EUl/DDz1OGHLl7vh9xteDtJFO5Oyljmq52FLWfl5MB80W11PQKBgDVb",
    "wv9IhhbxKW4VGGvCi9d0I7X0JhBYeIjYmqciUbraZft+gz/IoNmSUfyfZKVby6su",
    "XZ6hSr4dzgzVf2Pj9JblcAXHXCmEYUIwkkWgwIL55Zjtdbi90NZArK1Dfb//+lCO",
    "1YPvfs70FLItB0q4ISe4WpmoOnab3SrANgFvizMRAoGBALDG/5U73qFA7U9CQ4kj",
    "V7wx/REatRP7BX6A2iT5xP8moIwD04Dd8kOD2sNpbXPBHd4zN5ERrVFwVUnUqgAF",
    "QQKbistXdDfqDDJuqQaMg8/jP0cMZ8U+5Ext91+q2Cuf1nOcjCnxKn0yly4OFuHy",
    "6WvVAAKvi+6kqNAHrctlaiee",
);
const CLIENT_CERTIFICATE_DER_BASE64: &str = concat!(
    "MIIDOTCCAiGgAwIBAgIUJwMjsznGpAUETibtJVfn+DMmepQwDQYJKoZIhvcNAQEL",
    "BQAwFDESMBAGA1UEAwwJbG9jYWxob3N0MB4XDTI2MDcxNzEwMzM0MVoXDTM2MDcx",
    "NDEwMzM0MVowIjEgMB4GA1UEAwwXY29uY29yZC1sb29wYmFjay1jbGllbnQwggEi",
    "MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCMkXAzgK1TwlWKtcM22hMp+ht1",
    "Zi+CDgAxoZDZyq6cNz8YX+vnocW3FTZOOLrWHkoagwn0CyKCbXFa/at4OrfFuaDP",
    "yHUyQWSbNrPti8AKQD+/MVoM3U4OP0p+Uh3hp+253IU8Li2X5ZyIBwSPFViC6Zc",
    "F8UosZYcYp72fkFjJU7tS9cnjo87p6wYK27q1uHbggeWn8b3WdHpLRkRa789bjyN",
    "FpSfeQflgFPUHbytK5VHzQaYyfad4ddeaQHSQYrw+OX0V6zaRRC/xWAPKBkg+OUR",
    "dK2CL87cgerMCtBih+bq7IJC5BSti3f/zClvEAFsERfY6uK3tBZFPS8ZcCsyVAgM",
    "BAAGjdTBzMAwGA1UdEwEB/wQCMAAwDgYDVR0PAQH/BAQDAgeAMBMGA1UdJQQMMAoG",
    "CCsGAQUFBwMCMB0GA1UdDgQWBBSP4ytdMSHdY858yFfCCkpOvBmOKTAfBgNVHSME",
    "GDAWgBSAkEAV0bI0pbjKhqRoKnpVyRy99DANBgkqhkiG9w0BAQsFAAOCAQEATiXg",
    "D8GfQjnflV/koh0JrvDETbFh7tjOjgNOR8N1TrwdYO6phYPOx5lDvDHuxW3nCFeP",
    "ZF62ezy8q9pJH4y6Xs8eCELss3QPtdXnxTENhtyPFTnFpskJgj1I2taR55u9frdk",
    "mdtPdn58cJvzD2EuKplEbBDHB0Ru5Zb0LuMoMgR21ZjQoght93xw8bFSuWlW33h",
    "N4eNSLIbdGuQpmEBevGIBs2iPVnRxfQqfLJEq3pCjhQ7HN+wd4XOwes/XNvHIcwV",
    "m9cdU2CyEaXdCgfrKu19vj33ylpd4ct0UUFRf9dDrunICb4O2MhXPie5uHqfDhbf",
    "RicKxIYbD6ryFKArOww==",
);
const CLIENT_PRIVATE_KEY_DER_BASE64: &str = concat!(
    "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCMkXAzgK1TwlWK",
    "tcM22hMp+ht1Zi+CDgAxoZDZyq6cNz8YX+vnocW3FTZOOLrWHkoagwn0CyKCbXFa",
    "/at4OrfFuaDPyHUyQWSbNrPti8AKQD+/MVoM3U4OP0p+Uh3hp+253IU8Li2X5ZyI",
    "BwSPFViC6ZcF8UosZYcYp72fkFjJU7tS9cnjo87p6wYK27q1uHbggeWn8b3WdHpL",
    "RkRa789bjyNFpSfeQflgFPUHbytK5VHzQaYyfad4ddeaQHSQYrw+OX0V6zaRRC/x",
    "WAPKBkg+OURdK2CL87cgerMCtBih+bq7IJC5BSti3f/zClvEAFsERfY6uK3tBZFP",
    "S8ZcCsyVAgMBAAECggEABGWgPtwtj8sqZdZdrt+Pbea26SszHTxW4TaOk2F1qMlJ",
    "nfeSJ4K17o4pTrUTkxMLelzS+OFISRAnVZNNw8sCaJrv3tAEangFL528bxfKAAgW",
    "dmKS0Igzotpz+BOVf7ODCidNu54uKuwxw0eE6vzd8epgoPz1NwJOAYVttwoK3PFH",
    "sDEQLh1nBe2oDtO0DexMzyivOXKdQ5PFji5m24Q+JSJ8wjDIrxu5F+Jv3DxPDizU",
    "cvWXfSpD5CTvMqkk8zumJdg71uvXMMB6OF/I+O8+OVQ5KawLdC76UTgMXIPDDS7G",
    "ucZ07rRHz9KXGtDd+/H09q1wcaeGr9SiZA+x9jN0mQKBgQDFd6kcS/qARezuHtOz",
    "UBdoKOa/DvUoC5D6xvC/JhGrMSAg3GV0HVzNclE1pQ1ld3BlTLe3akSFVP2dWk+l",
    "bxpg4vxl3Eh7yHwZYqyF/hZEtUXDO8l0MDFf4WUEjIiPRycpHe7EjSXD2DXB7wO4",
    "aG10/Haki5RZTwYXKH36v8/8swKBgQC2PBxtwewZhq+r94zKAO7j06RYCGXksQjR",
    "8TuqbiybYI0+RJeB0NQput+AnI3RgQ0ym8tQF60jVhEpETGLZiP4RDai8b7wKHrK",
    "xl1mOs1nHb/+RXJEPnvY2GOoZau3MSKAj89OJjoAYSXA9kr4rECdhaL44lh8qJKO",
    "S4Hd2v3FlwKBgQC6tGo77Aa88+CSQKlvmGGZRVru+OiJZW49/o2wXgFEuN0k1N5E",
    "DGMriVQJgbaJ4oVt8jyeeiaxXmuxNKurwsHRK6zzbnVGPIhBxE9VHanWawK8FrJi",
    "XhLBEi7ePzX7Ko53TOruAnrbknk5lZER8gLUIsTWzXaFfrdGfC2pQWUZCwKBgGSB",
    "mKhMKNwXNXIh0MtkYlzS0DQhRZkwro2jf2XK5902DhBGokzdkPYG89eqAGmIxG+i",
    "Dyr4M+xmPoaozet1BOebL4sEy2sG2FpXtxF34us5NDeXs6LB12pMRgKNOHyLo9zF",
    "8+7FFH47FihrbgPidKysFKsxot311waVzGVAr2+3AoGABxwyfaSxX1NOjjsi3fdB",
    "EqpjxH02rWpcp0WLzi4UaeQz/Y2D72gIr3bZ6CDwiwrhju6wMEfxv78/ebY12nJc",
    "Wg35NoYsMPSAd7/noyIB0sHiUhoCScc8etyQQDfXy+tZ7o0RjbUa8f7BOV0YkmOv",
    "l89UcyF8w344P3u2p1mEvRM=",
);

#[derive(Clone)]
pub struct TlsRequest {
    connection_id: u64,
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl TlsRequest {
    #[must_use]
    pub fn connection_id(&self) -> u64 {
        self.connection_id
    }

    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

struct TlsResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    wire_parts: Option<Vec<(Duration, Vec<u8>)>>,
}

struct Shared {
    requests: Mutex<Vec<TlsRequest>>,
    responses: Mutex<VecDeque<TlsResponse>>,
    changed: Condvar,
    stop: AtomicBool,
    next_connection: AtomicU64,
}

pub struct TlsServer {
    address: SocketAddr,
    certificate: Vec<u8>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl TlsServer {
    pub fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::start_with_client_auth(false)
    }

    /// Starts the fixed loopback server with a mandatory client-certificate
    /// verifier rooted in the same non-secret fixture certificate.
    pub fn start_mtls() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Self::start_with_client_auth(true)
    }

    /// Starts the loopback server with a caller-supplied, non-secret test
    /// identity instead of the fixed `localhost` fixture.
    ///
    /// `root_certificate_pem` is the trust anchor returned by
    /// [`Self::certificate_pem`] for the client; `certificate_chain_pem` holds
    /// the server chain (leaf first) and `private_key_pem` its PKCS#8 key.
    /// This lets a fixed-HTTPS declaration whose logical host is not
    /// `localhost` (for example `eu.api.catalog.example.test`) complete a real
    /// verified TLS handshake through [`crate::proxy::ConnectProxy`]. The
    /// material must be owned test fixture material, never a real credential.
    pub fn start_with_identity_pem(
        root_certificate_pem: &str,
        certificate_chain_pem: &str,
        private_key_pem: &str,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let certificate = single_pem_block(root_certificate_pem, "CERTIFICATE")?;
        let certificate_chain = pem_blocks(certificate_chain_pem, "CERTIFICATE")?
            .into_iter()
            .map(CertificateDer::from)
            .collect::<Vec<_>>();
        if certificate_chain.is_empty() {
            return Err("the TLS identity chain holds no certificate".into());
        }
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(single_pem_block(
            private_key_pem,
            "PRIVATE KEY",
        )?));
        Self::start_with_identity(certificate, certificate_chain, key, false)
    }

    fn start_with_client_auth(
        require_client_auth: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let certificate = STANDARD.decode(ROOT_CERTIFICATE_DER_BASE64)?;
        let server_certificate = STANDARD.decode(SERVER_CERTIFICATE_DER_BASE64)?;
        let private_key = STANDARD.decode(SERVER_PRIVATE_KEY_DER_BASE64)?;
        let certificate_chain = vec![CertificateDer::from(server_certificate)];
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(private_key));
        Self::start_with_identity(certificate, certificate_chain, key, require_client_auth)
    }

    fn start_with_identity(
        certificate: Vec<u8>,
        certificate_chain: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
        require_client_auth: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let builder = ServerConfig::builder();
        let config = if require_client_auth {
            let mut roots = RootCertStore::empty();
            roots.add(CertificateDer::from(certificate.clone()))?;
            let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()?;
            builder
                .with_client_cert_verifier(verifier)
                .with_single_cert(certificate_chain, key)?
        } else {
            builder
                .with_no_client_auth()
                .with_single_cert(certificate_chain, key)?
        };
        let config = Arc::new(config);
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let shared = Arc::new(Shared {
            requests: Mutex::new(Vec::new()),
            responses: Mutex::new(VecDeque::new()),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
            next_connection: AtomicU64::new(1),
        });
        let thread_shared = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("concord-tls-probe".into())
            .spawn(move || accept_loop(listener, config, thread_shared))?;
        Ok(Self {
            address,
            certificate,
            shared,
            worker: Some(worker),
        })
    }

    #[must_use]
    pub fn url(&self, host: &str, path: &str) -> String {
        format!("https://{host}:{}{}", self.address.port(), path)
    }

    /// Queues one response status for the next request; an empty queue defaults to `200`.
    pub fn queue_status(&self, status: u16) -> io::Result<()> {
        self.queue_response(status, b"ok".to_vec())
    }

    /// Queues one status and complete response body for the next request.
    pub fn queue_response(&self, status: u16, body: impl Into<Vec<u8>>) -> io::Result<()> {
        self.queue_response_with_headers(status, &[], body)
    }

    /// Queues one status, extra response headers (for example `Location` or
    /// `Set-Cookie`) and a complete body for the next request. Header names
    /// and values must be single-line visible ASCII.
    pub fn queue_response_with_headers(
        &self,
        status: u16,
        headers: &[(&str, &str)],
        body: impl Into<Vec<u8>>,
    ) -> io::Result<()> {
        if headers.iter().any(|(name, value)| {
            name.is_empty()
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && byte != b':')
                || !value
                    .bytes()
                    .all(|byte| byte == b' ' || byte.is_ascii_graphic())
        }) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TLS probe headers must be single-line visible ASCII",
            ));
        }
        if !(200..=599).contains(&status) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "TLS probe status must be a final HTTP status",
            ));
        }
        self.shared
            .responses
            .lock()
            .map_err(|_| io::Error::other("TLS response queue mutex poisoned"))?
            .push_back(TlsResponse {
                status,
                headers: headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                    .collect(),
                body: body.into(),
                wire_parts: None,
            });
        Ok(())
    }

    /// Queues exact HTTP wire parts, each sent after its finite delay. The
    /// connection closes after the last part, allowing truncated-body probes.
    pub fn queue_wire_response(&self, parts: Vec<(Duration, Vec<u8>)>) -> io::Result<()> {
        if parts.len() > 16
            || parts
                .iter()
                .any(|(delay, bytes)| *delay > Duration::from_secs(5) || bytes.len() > 1024 * 1024)
        {
            return Err(io::Error::other("TLS wire fixture exceeds its bounds"));
        }
        self.shared
            .responses
            .lock()
            .map_err(|_| io::Error::other("TLS response queue mutex poisoned"))?
            .push_back(TlsResponse {
                status: 200,
                headers: Vec::new(),
                body: Vec::new(),
                wire_parts: Some(parts),
            });
        Ok(())
    }

    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate
    }

    /// Returns the fixed loopback certificate encoded as PEM.
    #[must_use]
    pub fn certificate_pem(&self) -> Vec<u8> {
        let encoded = STANDARD.encode(&self.certificate);
        format!("-----BEGIN CERTIFICATE-----\n{encoded}\n-----END CERTIFICATE-----\n").into_bytes()
    }

    /// Returns the fixed, non-secret loopback certificate and key as an
    /// identity PEM suitable for construction probes.
    #[must_use]
    pub fn identity_pem(&self) -> Vec<u8> {
        let certificate = STANDARD
            .decode(CLIENT_CERTIFICATE_DER_BASE64)
            .expect("embedded client certificate fixture must remain valid base64");
        let certificate = STANDARD.encode(certificate);
        let key = STANDARD
            .decode(CLIENT_PRIVATE_KEY_DER_BASE64)
            .expect("embedded client key fixture must remain valid base64");
        let key = STANDARD.encode(key);
        format!(
            "-----BEGIN CERTIFICATE-----\n{certificate}\n-----END CERTIFICATE-----\n\
             -----BEGIN PRIVATE KEY-----\n{key}\n-----END PRIVATE KEY-----\n"
        )
        .into_bytes()
    }

    #[must_use]
    pub fn requests(&self) -> Vec<TlsRequest> {
        self.shared
            .requests
            .lock()
            .expect("TLS request record mutex poisoned")
            .clone()
    }

    pub fn wait_for_requests(&self, expected: usize, timeout: Duration) -> Result<(), String> {
        let requests = self
            .shared
            .requests
            .lock()
            .expect("TLS request record mutex poisoned");
        let (requests, _) = self
            .shared
            .changed
            .wait_timeout_while(requests, timeout, |requests| requests.len() < expected)
            .expect("TLS request record mutex poisoned while waiting");
        if requests.len() >= expected {
            Ok(())
        } else {
            Err(format!(
                "TLS loopback server observed {} of {expected} requests",
                requests.len()
            ))
        }
    }
}

impl Drop for TlsServer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect(self.address);
        if let Some(worker) = self.worker.take() {
            worker.join().expect("TLS loopback server thread panicked");
        }
    }
}

fn accept_loop(listener: TcpListener, config: Arc<ServerConfig>, shared: Arc<Shared>) {
    let mut workers = Vec::new();
    while !shared.stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((tcp, _)) => {
                if shared.stop.load(Ordering::Acquire) {
                    break;
                }
                let config = Arc::clone(&config);
                let shared = Arc::clone(&shared);
                let connection_id = shared.next_connection.fetch_add(1, Ordering::Relaxed);
                workers.push(
                    thread::Builder::new()
                        .name("concord-tls-connection".into())
                        .spawn(move || handle_tls(tcp, connection_id, config, shared))
                        .expect("failed to spawn TLS connection worker"),
                );
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => panic!("TLS loopback accept failed: {error}"),
        }
    }
    for worker in workers {
        worker.join().expect("TLS connection worker panicked");
    }
}

fn handle_tls(tcp: TcpStream, connection_id: u64, config: Arc<ServerConfig>, shared: Arc<Shared>) {
    tcp.set_nonblocking(false)
        .expect("failed to make accepted TLS socket blocking");
    tcp.set_read_timeout(Some(Duration::from_secs(2)))
        .expect("failed to set TLS read timeout");
    // Small TLS records must not wait on Nagle/delayed-ACK interaction.
    tcp.set_nodelay(true)
        .expect("failed to disable Nagle on the TLS loopback connection");
    let connection = ServerConnection::new(config).expect("failed to create TLS connection");
    let mut stream = BufReader::new(StreamOwned::new(connection, tcp));
    loop {
        let Some(request) = read_request(&mut stream, connection_id) else {
            return;
        };
        {
            let mut requests = shared
                .requests
                .lock()
                .expect("TLS request record mutex poisoned");
            requests.push(request);
            shared.changed.notify_all();
        }
        let response = shared
            .responses
            .lock()
            .expect("TLS response queue mutex poisoned")
            .pop_front()
            .unwrap_or_else(|| TlsResponse {
                status: 200,
                headers: Vec::new(),
                body: b"ok".to_vec(),
                wire_parts: None,
            });
        if let Some(parts) = response.wire_parts {
            for (delay, bytes) in parts {
                std::thread::sleep(delay);
                if stream.get_mut().write_all(&bytes).is_err() || stream.get_mut().flush().is_err()
                {
                    return;
                }
            }
            stream.get_mut().conn.send_close_notify();
            let _ = stream.get_mut().flush();
            return;
        }
        let extra_headers: String = response
            .headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect();
        let head = format!(
            "HTTP/1.1 {} Scripted\r\n{extra_headers}Content-Length: {}\r\nConnection: keep-alive\r\n\r\n",
            response.status,
            response.body.len()
        );
        if stream.get_mut().write_all(head.as_bytes()).is_err()
            || stream.get_mut().write_all(&response.body).is_err()
        {
            return;
        }
        if stream.get_mut().flush().is_err() {
            return;
        }
    }
}

/// Parses one bounded HTTP/1 request. The connection-owned BufReader retains
/// read-ahead for the next request. Unsupported/ambiguous framing closes the
/// fixture connection without recording a successful empty-body request.
fn read_request(stream: &mut impl io::BufRead, connection_id: u64) -> Option<TlsRequest> {
    const MAX_HEAD_BYTES: usize = 16 * 1024;
    const MAX_BODY_BYTES: usize = 1024 * 1024;
    const MAX_TRAILER_BYTES: usize = 8 * 1024;
    // Counts all consumed framing too, bounding tiny chunks and extensions.
    let mut wire_budget = MAX_BODY_BYTES + 64 * 1024;
    let mut head_budget = MAX_HEAD_BYTES;
    let first = bounded_line(stream, &mut head_budget, &mut wire_budget)?;
    let mut parts = std::str::from_utf8(&first).ok()?.split_whitespace();
    let method = parts.next()?.to_owned();
    let target = parts.next()?.to_owned();
    if !matches!(parts.next()?, "HTTP/1.0" | "HTTP/1.1") || parts.next().is_some() {
        return None;
    }
    let mut headers = Vec::new();
    loop {
        let line = bounded_line(stream, &mut head_budget, &mut wire_budget)?;
        if line.is_empty() {
            break;
        }
        let (name, value) = std::str::from_utf8(&line).ok()?.split_once(':')?;
        if name.is_empty() || !name.bytes().all(header_name_byte) {
            return None;
        }
        if value
            .bytes()
            .any(|byte| byte < 0x20 && byte != b'\t' || byte == 0x7f)
        {
            return None;
        }
        headers.push((name.to_ascii_lowercase(), value.trim().to_owned()));
    }
    let lengths = headers
        .iter()
        .filter(|(name, _)| name == "content-length")
        .collect::<Vec<_>>();
    let encodings = headers
        .iter()
        .filter(|(name, _)| name == "transfer-encoding")
        .collect::<Vec<_>>();
    if lengths.len() > 1
        || encodings.len() > 1
        || !lengths.is_empty() && !encodings.is_empty()
        || headers.iter().any(|(name, _)| name == "expect")
    {
        return None;
    }
    let body = if let Some((_, encoding)) = encodings.first() {
        if !encoding.eq_ignore_ascii_case("chunked") {
            return None;
        }
        let mut body = Vec::new();
        loop {
            let mut line_budget = MAX_HEAD_BYTES;
            let line = bounded_line(stream, &mut line_budget, &mut wire_budget)?;
            let line = std::str::from_utf8(&line).ok()?;
            let (size, extension) = line
                .split_once(';')
                .map_or((line, None), |(a, b)| (a, Some(b)));
            if size.is_empty()
                || !size.bytes().all(|byte| byte.is_ascii_hexdigit())
                || extension
                    .is_some_and(|value| value.bytes().any(|byte| !(0x20..=0x7e).contains(&byte)))
            {
                return None;
            }
            let size = usize::from_str_radix(size, 16).ok()?;
            if size == 0 {
                let mut trailer_budget = MAX_TRAILER_BYTES;
                loop {
                    let line = bounded_line(stream, &mut trailer_budget, &mut wire_budget)?;
                    if line.is_empty() {
                        break;
                    }
                    let (name, value) = std::str::from_utf8(&line).ok()?.split_once(':')?;
                    if name.is_empty()
                        || !name.bytes().all(header_name_byte)
                        || name.eq_ignore_ascii_case("content-length")
                        || name.eq_ignore_ascii_case("transfer-encoding")
                        || value
                            .bytes()
                            .any(|byte| byte < 0x20 && byte != b'\t' || byte == 0x7f)
                    {
                        return None;
                    }
                }
                break;
            }
            let total = body.len().checked_add(size)?;
            if total > MAX_BODY_BYTES {
                return None;
            }
            wire_budget = wire_budget.checked_sub(size.checked_add(2)?)?;
            let start = body.len();
            body.resize(total, 0);
            stream.read_exact(&mut body[start..]).ok()?;
            let mut ending = [0_u8; 2];
            stream.read_exact(&mut ending).ok()?;
            if ending != *b"\r\n" {
                return None;
            }
        }
        body
    } else {
        let length = match lengths.first() {
            Some((_, value))
                if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) =>
            {
                value.parse::<usize>().ok()?
            }
            Some(_) => return None,
            None => 0,
        };
        if length > MAX_BODY_BYTES {
            return None;
        }
        wire_budget.checked_sub(length)?;
        let mut body = vec![0; length];
        stream.read_exact(&mut body).ok()?;
        body
    };
    Some(TlsRequest {
        connection_id,
        method,
        target,
        headers,
        body,
    })
}

fn header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// No read_until allocation: both line and total framing budgets apply before
/// retaining each byte. Every accepted line ends with an exact CRLF.
fn bounded_line(
    stream: &mut impl io::BufRead,
    line_budget: &mut usize,
    wire_budget: &mut usize,
) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    loop {
        *line_budget = line_budget.checked_sub(1)?;
        *wire_budget = wire_budget.checked_sub(1)?;
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).ok()?;
        line.push(byte[0]);
        if byte[0] == b'\n' {
            if line.len() < 2 || line[line.len() - 2] != b'\r' {
                return None;
            }
            line.truncate(line.len() - 2);
            return Some(line);
        }
    }
}

#[cfg(test)]
mod s5_framing_regressions {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn content_length_and_chunked_preserve_next_request() {
        for wire in [
            b"POST /one HTTP/1.1\r\nContent-Length: 5\r\n\r\nhelloGET /two HTTP/1.1\r\n\r\n".as_slice(),
            b"POST /one HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n2;fixture=yes\r\nhe\r\n3\r\nllo\r\n0\r\nX-Fixture: yes\r\n\r\nGET /two HTTP/1.1\r\n\r\n".as_slice(),
        ] {
            let mut reader = BufReader::new(Cursor::new(wire));
            let first = read_request(&mut reader, 7).expect("first request");
            assert_eq!(first.body, b"hello");
            let second = read_request(&mut reader, 7).expect("buffered next request");
            assert_eq!(second.target, "/two");
            assert_eq!(second.connection_id, 7);
        }
    }

    #[test]
    fn malformed_or_over_budget_framing_never_records_empty_success() {
        for wire in [
            "POST / HTTP/1.1\r\nContent-Length: 5\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\nhello",
            "POST / HTTP/1.1\r\nTransfer-Encoding: gzip, chunked\r\n\r\n0\r\n\r\n",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nxXX0\r\n\r\n",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n100001\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhell",
        ] {
            assert!(read_request(&mut BufReader::new(Cursor::new(wire)), 0).is_none());
        }
        let long = format!("POST / HTTP/1.1\r\nX: {}\r\n\r\n", "x".repeat(16 * 1024));
        assert!(read_request(&mut BufReader::new(Cursor::new(long)), 0).is_none());
    }
}

/// Decodes every `-----BEGIN {label}-----` block of fixture PEM text.
fn pem_blocks(
    pem: &str,
    label: &str,
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut blocks = Vec::new();
    let mut rest = pem;
    while let Some(start) = rest.find(&begin) {
        let body_start = start + begin.len();
        let Some(length) = rest[body_start..].find(&end) else {
            return Err(format!("unterminated PEM {label} block").into());
        };
        let encoded: String = rest[body_start..body_start + length]
            .chars()
            .filter(|character| !character.is_ascii_whitespace())
            .collect();
        blocks.push(STANDARD.decode(encoded)?);
        rest = &rest[body_start + length + end.len()..];
    }
    Ok(blocks)
}

/// Decodes exactly one PEM block with `label`.
fn single_pem_block(
    pem: &str,
    label: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let mut blocks = pem_blocks(pem, label)?;
    if blocks.len() != 1 {
        return Err(format!("expected exactly one PEM {label} block").into());
    }
    Ok(blocks.remove(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pem_identity_round_trips_the_fixed_fixture() {
        let fixed = TlsServer::start().expect("fixed fixture server");
        let root = String::from_utf8(fixed.certificate_pem()).expect("ASCII PEM");
        assert_eq!(
            single_pem_block(&root, "CERTIFICATE").expect("one block"),
            fixed.certificate_der()
        );
        let twice = format!("{root}{root}");
        assert_eq!(
            pem_blocks(&twice, "CERTIFICATE").expect("two blocks").len(),
            2
        );
        assert!(single_pem_block(&twice, "CERTIFICATE").is_err());
        assert!(pem_blocks("-----BEGIN CERTIFICATE-----\nAAAA", "CERTIFICATE").is_err());

        let identity = String::from_utf8(fixed.identity_pem()).expect("ASCII PEM");
        let custom = TlsServer::start_with_identity_pem(&root, &identity, &identity)
            .expect("caller-supplied identity server");
        assert_eq!(custom.certificate_der(), fixed.certificate_der());
    }
}
