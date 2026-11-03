Test-only TLS material for `tests/tls.rs` (loopback HTTPS). Not secret; the CA key was discarded.

- `ca.pem` — test CA certificate (the client's only trust root).
- `cert.pem`, `key.pem` — `localhost`/`127.0.0.1` server certificate signed by that CA, and its key.

Regenerate with openssl: a CA (`basicConstraints=critical,CA:TRUE`) and a leaf with
`CA:FALSE`, `extendedKeyUsage=serverAuth`, `subjectAltName=DNS:localhost,IP:127.0.0.1`.
