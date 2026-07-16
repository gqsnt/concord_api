use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

// Fixed, non-secret test fixture material. This key authenticates only the
// loopback probe server and is never used as an application credential.
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

#[derive(Clone)]
pub struct TlsRequest {
    target: String,
    headers: Vec<(String, String)>,
}

impl TlsRequest {
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
}

struct Shared {
    requests: Mutex<Vec<TlsRequest>>,
    changed: Condvar,
    stop: AtomicBool,
}

pub struct TlsServer {
    address: SocketAddr,
    certificate: Vec<u8>,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl TlsServer {
    pub fn start() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let certificate = STANDARD.decode(CERTIFICATE_DER_BASE64)?;
        let private_key = STANDARD.decode(PRIVATE_KEY_DER_BASE64)?;
        let certificate_chain = vec![CertificateDer::from(certificate.clone())];
        let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(private_key));
        let config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certificate_chain, key)?,
        );
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let shared = Arc::new(Shared {
            requests: Mutex::new(Vec::new()),
            changed: Condvar::new(),
            stop: AtomicBool::new(false),
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

    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    #[must_use]
    pub fn certificate_der(&self) -> &[u8] {
        &self.certificate
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
                workers.push(
                    thread::Builder::new()
                        .name("concord-tls-connection".into())
                        .spawn(move || handle_tls(tcp, config, shared))
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

fn handle_tls(tcp: TcpStream, config: Arc<ServerConfig>, shared: Arc<Shared>) {
    tcp.set_nonblocking(false)
        .expect("failed to make accepted TLS socket blocking");
    tcp.set_read_timeout(Some(Duration::from_secs(2)))
        .expect("failed to set TLS read timeout");
    let connection = ServerConnection::new(config).expect("failed to create TLS connection");
    let mut stream = StreamOwned::new(connection, tcp);
    let Some(request) = read_request(&mut stream) else {
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
    let _ =
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    let _ = stream.flush();
}

fn read_request(stream: &mut impl Read) -> Option<TlsRequest> {
    let mut bytes = Vec::new();
    let mut block = [0_u8; 1024];
    while bytes.windows(4).all(|window| window != b"\r\n\r\n") {
        let count = stream.read(&mut block).ok()?;
        if count == 0 || bytes.len() + count > 16 * 1024 {
            return None;
        }
        bytes.extend_from_slice(&block[..count]);
    }
    let end = bytes.windows(4).position(|window| window == b"\r\n\r\n")?;
    let text = std::str::from_utf8(&bytes[..end]).ok()?;
    let mut lines = text.split("\r\n");
    let target = lines.next()?.split_whitespace().nth(1)?.to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Some(TlsRequest { target, headers })
}
