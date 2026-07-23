use std::collections::VecDeque;
use std::io::{self, Read, Write};
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
const CERTIFICATE_DER_BASE64: &str = concat!(
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
const PRIVATE_KEY_DER_BASE64: &str = concat!(
    "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCncj2TyFtoymc1",
    "5ZQB0Y1VMaO/Z9t/p7BSRtBBM7JBl24Mfks3UxtUugbVDQE9V4J0TK8/PpjF2AUm",
    "Hw2O3inLFMVg5GiyGRuIusYED/bpwX9h+Rl9a9R8lSzjBQ7hf344hUn7b2xmM1dU",
    "JsJy7G00cOzPEYGo7vn3LUjWxvQ/i0/eN/VQ0oJsStScfeW5sHXyjiA/Uz2VtnAk",
    "Tz+VdMgfPtwgG8B23tslQsYHVvr+VPMRKQIghoKVSVWr+3qe1AjvsJEAyeldYLrf",
    "C+v2M+D7a3h4rC5mhAUqJqrfZiw/pNs95lesw5C0+gIjg6N4jhOf587Ed6o86Jc3",
    "sf99AfrXAgMBAAECggEACNEcSptKq45jd97iTQ7hKm1mNLkTFuTYBRpjHdkggjv8",
    "g0jlIgEiF8iB0pcQNnrUoitlhjwquYQsj+YjsHRPRA5lxDp9WXS2p/DvEJ4HP3tn",
    "qdt+yvAHbMCYGqNqC1f2EqN7Z/NlLvkgwza/7Lg5RIEhCa2KI8CNm0UgUuV/9+vX",
    "/nsV8dCxrjF+2KKtZBGk5A44F9JtQoLoIplEdiA0qC+RDd7LWJCiEzmI5S+QHM1R",
    "1ERiuK8VyhoTPW1et7eR0HBGsQ4hLPgPVxrEavKa9QTq4zWRK7KoSOTfuVq0UMUY",
    "xtYShRqU7IpBFvUxMKzYU3/J36SkoAdx1OkTijTx2QKBgQDWswPMfOJXM+utsxYT",
    "ZwSqx6HhpYxkkfVAFA/kx6zDYO39U2swUXSBetpTt48nR/1JLO/h+oZCPEnKU/iI",
    "z5U68rL2L5Dyca+8pRzdWgiE+8sJEXmsJO7aOzb619ymoXAlgxVrFcqzPExoIy+7",
    "XksaR6UrHGzkMkzlWRZqwduUjwKBgQDHqDloTOxvZ5Ckjxu7Bd/pW8cg756iJag9",
    "BUzQl7/5JDq+0XLjDu0zwVwvSzvRXFlGC2B7R48u3Pu8SxhcZQDlG87L9wh0hVkc",
    "Qk8J1Xm+LFHxJ9WWsv/iU8EXN1elCYiJE0r7Q+oc1CW43wjuh4iSYA23vapv1ozE",
    "686fuf0pOQKBgF2L1Ezl4QzTz414aVb83Pi4UN1rb1dmjTBlTWOGaM0cT7XAAgUk",
    "iJnd+IDfit03rbGThdW1fYggkpo4qiKaJ9BubPF9Wi94v3R3kLMVhn7C4v1JgvUx",
    "hmzJvjPfmAF2d9GwYsWs0gv0auo44pRGCn0h2GKHWR64l+6aOX08iDsbAoGAbLg/",
    "InuY2AU2L1W6TPoi9qjCYz34TOVimm/TZ1bq85j14EifXeMX6XHmZ9544UV/W3QA",
    "gsDxyBwPfHYL+uwJM7kTv99yHDks0AWPkpRLafIqYL5HQXuReUYQRDsfn7/0OGho",
    "ETDnP/GJ5QSWkYT+Km7hqWc/cw0KtISPjEyz/YkCgYEAt5/rGKWOhat6WTGH/kyJ",
    "3ofVpbru10LfMw9y6MfEA51dJ/aB1OqPQB580D5KWz4tLmXaeOe9jg/dE4ht3RbQ",
    "Fo9S3xXOFr8fFdolVPtLOWjh9xT8E2mmdnf7hqRjiTl0Qb/DrBhYIwlgGIlq0lj8",
    "MvwsZD+/zLGkhj7G71aUBrI=",
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
    body: Vec<u8>,
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

    fn start_with_client_auth(
        require_client_auth: bool,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let certificate = STANDARD.decode(CERTIFICATE_DER_BASE64)?;
        let private_key = STANDARD.decode(PRIVATE_KEY_DER_BASE64)?;
        let certificate_chain = vec![CertificateDer::from(certificate.clone())];
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(private_key));
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
                body: body.into(),
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
    let connection = ServerConnection::new(config).expect("failed to create TLS connection");
    let mut stream = StreamOwned::new(connection, tcp);
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
                body: b"ok".to_vec(),
            });
        let head = format!(
            "HTTP/1.1 {} Scripted\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
            response.status,
            response.body.len()
        );
        if stream.write_all(head.as_bytes()).is_err() || stream.write_all(&response.body).is_err() {
            return;
        }
        if stream.flush().is_err() {
            return;
        }
    }
}

fn read_request(stream: &mut impl Read, connection_id: u64) -> Option<TlsRequest> {
    const MAX_HEAD_BYTES: usize = 16 * 1024;
    const MAX_BODY_BYTES: usize = 1024 * 1024;

    let mut bytes = Vec::new();
    let mut block = [0_u8; 1024];
    while bytes.windows(4).all(|window| window != b"\r\n\r\n") {
        let count = stream.read(&mut block).ok()?;
        if count == 0 || bytes.len() + count > MAX_HEAD_BYTES {
            return None;
        }
        bytes.extend_from_slice(&block[..count]);
    }
    let end = bytes.windows(4).position(|window| window == b"\r\n\r\n")?;
    let text = std::str::from_utf8(&bytes[..end]).ok()?;
    let mut lines = text.split("\r\n");
    let mut request_line = lines.next()?.split_whitespace();
    let method = request_line.next()?.to_owned();
    let target = request_line.next()?.to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<Vec<_>>();
    let body_length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .map_or(Some(0), |(_, value)| value.parse::<usize>().ok())?;
    if body_length > MAX_BODY_BYTES {
        return None;
    }
    let body_start = end + 4;
    let total = body_start.checked_add(body_length)?;
    while bytes.len() < total {
        let count = stream.read(&mut block).ok()?;
        if count == 0 || bytes.len() + count > total {
            return None;
        }
        bytes.extend_from_slice(&block[..count]);
    }
    Some(TlsRequest {
        connection_id,
        method,
        target,
        headers,
        body: bytes[body_start..total].to_vec(),
    })
}
