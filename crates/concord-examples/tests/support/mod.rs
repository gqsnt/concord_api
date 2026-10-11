//! Shared TLS loopback harness for the canonical Catalog tests.
//!
//! The canonical Catalog keeps its declared fixed base
//! `https://catalog.example.test/root/` and generated hosts such as
//! `eu.api.catalog.example.test`. The CONNECT proxy tunnels every destination
//! to one loopback TLS server presenting the fixture certificate below, so the
//! client performs a real verified handshake for the declared logical host and
//! credentials are only ever sent over HTTPS. No public host is resolved.
//!
//! The certificate authority, leaf certificate and key are non-secret test
//! fixture material generated offline for this suite (valid 2026-09-25 to
//! 2036-09-22). They authenticate nothing outside this loopback harness.

#![allow(dead_code)]

use concord::SecretString;
use concord::http::SafeProxy;
use concord_examples::application::construction::catalog_builder;
use concord_examples::contracts::catalog_v3::CatalogApi;
use concord_test_support::proxy::ConnectProxy;
use concord_test_support::tls::TlsServer;

/// Fixture trust anchor.
pub const CATALOG_TEST_ROOT_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIDSzCCAjOgAwIBAgIUB+RZQIcjHQ9lEJDujE4KgTDMltMwDQYJKoZIhvcNAQEL
BQAwLTErMCkGA1UEAwwiQ29uY29yZCBFeGFtcGxlcyBDYXRhbG9nIFRlc3QgUm9v
dDAeFw0yNjA5MjUxNjE3MDRaFw0zNjA5MjIxNjE3MDRaMC0xKzApBgNVBAMMIkNv
bmNvcmQgRXhhbXBsZXMgQ2F0YWxvZyBUZXN0IFJvb3QwggEiMA0GCSqGSIb3DQEB
AQUAA4IBDwAwggEKAoIBAQCVLnt+T0p7ay4ut2raFieFKgY5rAgNW8qauR3v0Nle
tGA9eM0idpvr8llGnY1JbzayVlO9QBApD0Fc1t1nDyK31G+xsUEzA+DgcAiRIC8W
JQgPtakajv9leh4++/bOkH815/FlqTA3m08VWYrJe2Iaefs/WiM6wIT6jnZlq99V
D3clehmddUNpjUj0g3axf+JoT4cYky+0I+vE+isVVI3Hvkg00ETcC6R7SZGlOrUL
ChMa06AstGJGGEtblU4dp9/4Z5rpAW7gZUXyEOvoeRx9OAmNLddIY3une5vAc6h9
1FyoR6Q8CTKehB/wvTRhbwYhaDdIp5G1l//mHHaNfUxfAgMBAAGjYzBhMB0GA1Ud
DgQWBBQLzEzs6M98vpvdyfDMHyQFnhddETAfBgNVHSMEGDAWgBQLzEzs6M98vpvd
yfDMHyQFnhddETAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBBjANBgkq
hkiG9w0BAQsFAAOCAQEAk2t4tnch4MxnNsV6fhQJVFZInD8/kGTuhudCXWyVVv5g
XZ/9YHDerEM/6mEOXrtMPqvJdEgvm8tsPiqIPpsnchBdbessQ4ppyVwkfyHEpFTD
FmWkfhbdJ9zuI22koVOFSIKGifpml/Vg3x2p2K/xjUHg1DDWnEs0xrwf57LS5t3W
HLQWVtLZ257J3dc11q6ng+76MsXMZ0FiQIZecHMcnXsDOI0yVJYjg1NWmNqDG0Ut
KMuoFbpMC1rQL9JTP5MbLXGhOZqLcx0j0UsKsLRBsKpp+qK4gwLNosvH/Jrlos/I
3T8uxbNk5Dn5VMOUs9Cp/uWS0XrEAdAArdaaHX9+ig==
-----END CERTIFICATE-----
";

/// Fixture server certificate for `catalog.example.test`,
/// `api.catalog.example.test` and `*.api.catalog.example.test`.
pub const CATALOG_TEST_LEAF_PEM: &str = "-----BEGIN CERTIFICATE-----
MIIDrDCCApSgAwIBAgIUEpadxKDyxPG+zIqD4KVxy0D74hgwDQYJKoZIhvcNAQEL
BQAwLTErMCkGA1UEAwwiQ29uY29yZCBFeGFtcGxlcyBDYXRhbG9nIFRlc3QgUm9v
dDAeFw0yNjA5MjUxNjE3MTVaFw0zNjA5MjIxNjE3MTVaMCMxITAfBgNVBAMMGGFw
aS5jYXRhbG9nLmV4YW1wbGUudGVzdDCCASIwDQYJKoZIhvcNAQEBBQADggEPADCC
AQoCggEBAL1O4C5zw0FZe8LepPtVD76dBtGJFo9R91u3dkjSLTUKVAU5ug6gUXgG
ZtaaT1eODfH6llz7QCMJF01CRSM+k/nqtox0cgNuHGmDnVaOFDa0ie7qV5I2f46L
ijd3SmaxTkkm6X17sTGlAxLGanE7vc5YCoBs7MHYdXlCVpK2KpN9tWVjVK7gW6Cg
GOm9yAWJlAH/x8PrUrJa2sMaTELUprO83qmJvjPXKHKoR09EQbTmT4IU6z4AlXSv
RvGaieJceue1/ljnl1QoOD+kBAulHOFf8qTEJB/c5hqAGl0XiBS/JJQnAJwS5lnc
V59VNCZFaLdxKg3r00C9mCIC/cQw1ZECAwEAAaOBzTCByjAMBgNVHRMBAf8EAjAA
MA4GA1UdDwEB/wQEAwIFoDATBgNVHSUEDDAKBggrBgEFBQcDATBVBgNVHREETjBM
ghRjYXRhbG9nLmV4YW1wbGUudGVzdIIYYXBpLmNhdGFsb2cuZXhhbXBsZS50ZXN0
ghoqLmFwaS5jYXRhbG9nLmV4YW1wbGUudGVzdDAdBgNVHQ4EFgQURqypTGBJsGki
sQ3xcZZKy6pHzyswHwYDVR0jBBgwFoAUC8xM7OjPfL6b3cnwzB8kBZ4XXREwDQYJ
KoZIhvcNAQELBQADggEBACV7J+o/cwFbpP0du1QjX0ynvcC79Z6H3Ho5DjJv/HzG
1vTq53G/xUMs6gchzKJUdwqqUpZmy7hQyl7S9xbD4vQNpgOpPVXpA0AKhp/ldKNw
1pm93f/E+xrA7+ktEePh+LEDqIfP7aqOhypdhwvsn2OtTE1BS03aqKcwuSIu1oQN
0Wz7ODK4ZLqFRCDMQVAmXY3dARHEoWsiWjb28q5vSn5u7eAQxrNWMd0Aaw8NK/89
gkvjIHFqZ6suG2KGIfJaUvp53r6O9rM0alO+twqJNpjdj4dr+RFi/c0I4tRy8JYu
2JE4sFMVjtDXH62AR+g/mecjYiFZdT7YDN1FhLOsTJk=
-----END CERTIFICATE-----
";

/// Fixture server key (PKCS#8).
pub const CATALOG_TEST_KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC9TuAuc8NBWXvC
3qT7VQ++nQbRiRaPUfdbt3ZI0i01ClQFOboOoFF4BmbWmk9Xjg3x+pZc+0AjCRdN
QkUjPpP56raMdHIDbhxpg51WjhQ2tInu6leSNn+Oi4o3d0pmsU5JJul9e7ExpQMS
xmpxO73OWAqAbOzB2HV5QlaStiqTfbVlY1Su4FugoBjpvcgFiZQB/8fD61KyWtrD
GkxC1KazvN6pib4z1yhyqEdPREG05k+CFOs+AJV0r0bxmoniXHrntf5Y55dUKDg/
pAQLpRzhX/KkxCQf3OYagBpdF4gUvySUJwCcEuZZ3FefVTQmRWi3cSoN69NAvZgi
Av3EMNWRAgMBAAECggEAIyI3YwejDkbQPEtb7TVNNcAo2syJflHlCYWUE1HGiBiB
nXKoQDyqwg7WGTAMV6CKoAU00M7kiVwPkgwhjrf3eKOB+7FE07Qk28RMeBroT3rU
Op4NYJpys/7byQ1/pm1zrQnmvZ4uNZt9ZYeji55dzKT0Vf9hNVy5+KZCMPpUDokt
YI5TMp/JxxyT5D3OddAFsP6n35slrMBBUBwk/UqLfbefduOQNIrWOFRF50YCCDEB
fRSN05HE1B/bdk6zJHpMT0+He6pw/9Avd8kdcaPwd0ymlfbdGVAEOdHLKXF8YNbp
7K5K2RMgkXHyUPb0yim7IIicStlNr4R5XRmkKdS/rQKBgQDeRxNZztEYuMB6c2hk
9KfQPy01VWztaIMUhAHP5ODTd7/Yghn71VyjQ7MbyL+J1ANytT6r8XrK8Fg0D7kQ
rreWKH+BUGkn5EfVczaT+F0Tt7JHj16FsPIf6fDKnmOqVchbln/DLYlVCeldqm0Y
ROy7Eq3pQ7zh14PT8cjkt9+T5wKBgQDaB1BXzP2to7GBBUtKXUC9vbjFRtcFQEnv
MnUicXPBjIjWnj+vVwbj/6daTWspBnnidcinD2jM5cZ8jt3CdtF5+0O7SZL8EhXR
yETVG7E6iPqOl+x37w0s55aCly7UPyhmHlXcBDovJ/K6la6s9S7knSuL+4xRulbQ
d88md2ibxwKBgAxrkEs+YTSubiLpe/XYLeBXB2GpilKhumFVFvzU3zMxx8iDTIxj
2a9Nwx5UU0+SRzcLpZrZ/XCFiYgvMLob6So8zRMeuSF/gfwNWgleoWTOqOmSu0dh
IcZivg+0VgFYlb94Y182livoR9/x+/KVN10kqA8Wt+Eut7EKG52cIKQVAoGBAMwG
rBh9q6Yh2Klz2tDsEvl74Lw7Xlt7nbRI83QX1sr4R9UOoWnjdn9FbcYDTJRHiydw
MSSkNt4G54cH6l1f8wbDKs7IOtPH2MNHNaJ3F5I/PLjTUmWpPhsXXLE36IqVz6hA
GviYvBoEuBACokMX7Znhui50ZwrT5QmQFDla5HlhAoGAFzpMA8dmVdSaGqYELCAU
WpjfvQ84yLfHTO5FDC+PJP9T3Hq4/RafsGf2ZzmPFmLepFTzvxdXzgmgGQJSvwLL
0NdmXAZUvwysHGuHjH+e8PeCZ6vt5TaF17WDQ+yrUQbRbrN1Cq+/mn8O8r+p2Jkd
lfxPYFP6w8btaVwJ958jN8w=
-----END PRIVATE KEY-----
";

/// Owned fake bootstrap key sentinel; never a real provider credential.
pub const BOOTSTRAP_SENTINEL: &str = "fake-bootstrap-sentinel-7d1c";

/// Fake tenant sentinel (sent, but excluded from automatic observation).
pub const TENANT_SENTINEL: &str = "tenant-sentinel-51af";

/// A running Catalog loopback: TLS server, CONNECT tunnel and client.
pub struct CatalogHarness {
    /// The loopback TLS server presenting the Catalog fixture identity.
    pub server: TlsServer,
    /// Keeps the CONNECT tunnel alive for the client's lifetime.
    pub tunnel: ConnectProxy,
    /// The canonical Catalog built through the §23.1 builder.
    pub client: CatalogApi,
}

impl CatalogHarness {
    /// Starts the loopback and builds the Catalog through
    /// [`catalog_builder`], layering only the loopback transport (proxy,
    /// fixture trust root, HTTP/1.1) on top of the production configuration.
    pub fn start() -> Self {
        let server = TlsServer::start_with_identity_pem(
            CATALOG_TEST_ROOT_PEM,
            CATALOG_TEST_LEAF_PEM,
            CATALOG_TEST_KEY_PEM,
        )
        .expect("catalog TLS loopback");
        let tunnel = ConnectProxy::start(server.address()).expect("CONNECT tunnel");
        let proxy = SafeProxy::parse(tunnel.url()).expect("safe CONNECT proxy");
        let root = server.certificate_pem();
        let client = catalog_builder(
            TENANT_SENTINEL.to_owned(),
            SecretString::try_new(BOOTSTRAP_SENTINEL).expect("bounded fake bootstrap key"),
        )
        .configure_http(move |http| {
            http.http1_only()
                .proxy(proxy)
                .add_root_certificate_pem(root)
        })
        .build()
        .expect("canonical catalog client");
        Self {
            server,
            tunnel,
            client,
        }
    }

    /// Queues one JSON response.
    pub fn json(&self, status: u16, body: &str) {
        self.server
            .queue_response_with_headers(
                status,
                &[("content-type", "application/json")],
                body.as_bytes().to_vec(),
            )
            .expect("queue JSON response");
    }

    /// Queues a successful session token response for `authenticate`.
    pub fn token(&self, token: &str) {
        self.json(200, &format!("\"{token}\""));
    }
}
